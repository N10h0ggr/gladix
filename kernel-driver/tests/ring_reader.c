#include <windows.h>
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#pragma pack(push, 1)
typedef struct {
    volatile LONG  head;      // next-free byte for producer
    volatile LONG  tail;      // first unread byte for consumer
    volatile LONG  dropped;   // #events producer had to drop
    uint32_t       size;      // data area size in bytes
} Header;
#pragma pack(pop)

/* copy `len` bytes from circular buffer at offset `off` into `dst` */
static void copy_circular(const uint8_t *data, uint32_t size,
                          uint32_t off, void *dst, uint32_t len)
{
    uint32_t first = len;
    if (off + len > size)
        first = size - off;

    memcpy(dst, data + off, first);
    if (first < len)
        memcpy((uint8_t*)dst + first, data, len - first);
}

/* same as copy_circular but writes `len` bytes of zero to the buffer */
static void zero_circular(uint8_t *data, uint32_t size,
                          uint32_t off, uint32_t len)
{
    uint32_t first = len;
    if (off + len > size)
        first = size - off;

    memset(data + off, 0, first);
    if (first < len)
        memset(data, 0, len - first);
}

int main(void)
{
    /* open + map with write access */
    HANDLE hMap = OpenFileMappingA(FILE_MAP_ALL_ACCESS, FALSE,
                                   "Global\\GladixSharedSection");
    if (!hMap) {
        fprintf(stderr, "OpenFileMapping failed: %lu\n", GetLastError());
        return 1;
    }
    uint8_t *base = (uint8_t*)MapViewOfFile(hMap,
                        FILE_MAP_WRITE /* allow writes */,
                        0, 0, 0);
    if (!base) {
        fprintf(stderr, "MapViewOfFile failed: %lu\n", GetLastError());
        CloseHandle(hMap);
        return 1;
    }

    Header *hdr  = (Header*)base;
    uint8_t *data = base + sizeof(Header);
    uint32_t size = hdr->size;

    /* grab current pointers once */
    uint32_t tail = hdr->tail;
    uint32_t head = hdr->head;

    printf("head=%u  tail=%u  dropped=%u  size=%u\n\n",
           head, tail, hdr->dropped, size);

    for (int ev = 0; ev < 10; ++ev)
    {
        if (tail == head) {
            printf("ring empty (only %d event(s) present)\n", ev);
            break;
        }

        /* 1) read length */
        uint32_t len_le;
        copy_circular(data, size, tail, &len_le, 4);
        uint32_t msg_len = len_le;  // little-endian already
        uint32_t len_off = tail;    // remember where we read length
        tail = (tail + 4) % size;

        /* 2) read payload */
        uint8_t *msg = malloc(msg_len);
        if (!msg) { fprintf(stderr, "OOM\n"); break; }
        copy_circular(data, size, tail, msg, msg_len);
        uint32_t msg_off = tail;    // where payload started
        tail = (tail + msg_len) % size;

        /* 3) print it out */
        printf("/* Event %d — %u bytes */\n", ev + 1, msg_len);
        printf("const EVENT_%d: &[u8] = &[\n", ev + 1);
        for (uint32_t i = 0; i < msg_len; ++i) {
            printf("    0x%02X%s", msg[i],
                   (i + 1 < msg_len) ? "," : "");
            if ((i + 1) % 16 == 0) putchar('\n');
            else putchar(' ');
        }
        printf("\n];\n\n");
        free(msg);

        /* 4) zero out what we just consumed */
        zero_circular(data, size, len_off, 4);
        zero_circular(data, size, msg_off, msg_len);

        /* 5) advance the shared tail atomically */
        InterlockedExchange((LONG*)&hdr->tail, (LONG)tail);

        /* reload head in case producer moved */
        head = hdr->head;
    }

    UnmapViewOfFile(base);
    CloseHandle(hMap);
    return 0;
}
