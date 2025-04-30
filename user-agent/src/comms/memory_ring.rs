// src/comms/memory_ring.rs
use memmap2::{MmapMut, MmapOptions};
use std::{
    fs::OpenOptions,
    path::Path,
    sync::atomic::{AtomicUsize, Ordering},
};
use tokio::task::yield_now;

/// Un anillo de memoria mapeada por un driver y leído desde user-mode.
pub struct MemoryRing {
    mmap:        MmapMut,
    head:        *mut AtomicUsize,
    tail:        *mut AtomicUsize,
    data_offset: usize,
    buf_size:    usize,
}

// Permitir uso concurrente ya que accesos son atómicos y el mapping es seguro.
unsafe impl Send for MemoryRing {}
unsafe impl Sync for MemoryRing {}

impl MemoryRing {
    /// Abre (y mapea) el fichero de anillo.
    pub fn open<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)?;
        let metadata = file.metadata()?;
        let len = metadata.len() as usize;
        let header_bytes = 2 * std::mem::size_of::<AtomicUsize>();
        if len <= header_bytes {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "file too small for MemoryRing header",
            ));
        }

        let mmap = unsafe { MmapOptions::new().map_mut(&file)? };
        // Asumimos alineación de página al inicio
        let ptr = mmap.as_ptr() as *mut AtomicUsize;
        let head = ptr;
        let tail = unsafe { ptr.add(1) };

        Ok(MemoryRing { mmap, head, tail, data_offset: header_bytes, buf_size: len - header_bytes })
    }

    /// Extrae el siguiente evento (payload puro) si hay datos; espera (async) si está vacío.
    pub async fn pop(&self) -> Option<Vec<u8>> {
        loop {
            let h = unsafe { (*self.head).load(Ordering::Acquire) };
            let t = unsafe { (*self.tail).load(Ordering::Acquire) };
            if h == t {
                yield_now().await;
                continue;
            }

            let off = self.data_offset + h;
            let len_bytes = &self.mmap[off..off + 4];
            let payload_len = u32::from_le_bytes(len_bytes.try_into().unwrap()) as usize;
            let start = off + 4;
            let end = start + payload_len;
            let data = self.mmap[start..end].to_vec();

            let total = 4 + payload_len;
            let pad = (8 - (total % 8)) % 8;
            let mut new_h = h + total + pad;
            if new_h >= self.buf_size {
                new_h -= self.buf_size;
            }
            unsafe { (*self.head).store(new_h, Ordering::Release) };

            return Some(data);
        }
    }
}