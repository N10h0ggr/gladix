//! dispatch.rs
//! An extensible dispatcher for Windows IRP major functions.

use wdk_sys::{
    DRIVER_OBJECT, DEVICE_OBJECT, IRP, NTSTATUS,
    ntddk::IofCompleteRequest,
    IO_NO_INCREMENT,
    STATUS_SUCCESS, STATUS_NOT_IMPLEMENTED,
};

use crate::helpers::io_get_current_irp_stack_location;

/// Prototype for IRP handlers.
pub type DispatchFn = unsafe extern "C" fn(
    device_object: *mut DEVICE_OBJECT,
    irp: *mut IRP,
) -> NTSTATUS;

/// Default handler: completes IRP with STATUS_NOT_IMPLEMENTED.
unsafe extern "C" fn default_handler(
    _device_object: *mut DEVICE_OBJECT,
    irp: *mut IRP,
) -> NTSTATUS {
    unsafe {
        // Set status and information
        (*irp).IoStatus.__bindgen_anon_1.Status = STATUS_NOT_IMPLEMENTED;
        (*irp).IoStatus.Information = 0;
        // Complete the request
        IofCompleteRequest(irp, IO_NO_INCREMENT as i8);
    }
    STATUS_NOT_IMPLEMENTED
}

/// Dispatcher holds handlers for each major function (0–27).
pub struct Dispatcher {
    handlers: [DispatchFn; 28],
}

impl Dispatcher {
    /// All slots initialize to `default_handler`.
    pub const fn new() -> Self {
        Self { handlers: [default_handler; 28] }
    }

    /// Register a handler for a given IRP major code.
    pub fn register(&mut self, major_fn: u32, handler: DispatchFn) {
        let idx = major_fn as usize;
        if idx < self.handlers.len() {
            self.handlers[idx] = handler;
        }
    }

    /// Install all handlers into the driver object's MajorFunction array.
    pub unsafe fn install(&self, driver: *mut DRIVER_OBJECT) {
        for (i, &h) in self.handlers.iter().enumerate() {
            (*driver).MajorFunction[i] = Some(h);
        }
    }
}

// ------------------------------------------------------------
// Standard dispatch handlers

/// IRP_MJ_READ: succeed no data.
pub unsafe extern "C" fn dispatch_read(
    _device_object: *mut DEVICE_OBJECT,
    irp: *mut IRP,
) -> NTSTATUS {
    (*irp).IoStatus.__bindgen_anon_1.Status = STATUS_SUCCESS;
    (*irp).IoStatus.Information = 0;
    IofCompleteRequest(irp, IO_NO_INCREMENT as i8);
    STATUS_SUCCESS
}

/// IRP_MJ_WRITE: succeed no processing.
pub unsafe extern "C" fn dispatch_write(
    _device_object: *mut DEVICE_OBJECT,
    irp: *mut IRP,
) -> NTSTATUS {
    (*irp).IoStatus.__bindgen_anon_1.Status = STATUS_SUCCESS;
    (*irp).IoStatus.Information = 0;
    IofCompleteRequest(irp, IO_NO_INCREMENT as i8);
    STATUS_SUCCESS
}

/// IRP_MJ_CREATE: always succeed.
pub unsafe extern "C" fn dispatch_create(
    _device_object: *mut DEVICE_OBJECT,
    irp: *mut IRP,
) -> NTSTATUS {
    (*irp).IoStatus.__bindgen_anon_1.Status = STATUS_SUCCESS;
    (*irp).IoStatus.Information = 0;
    IofCompleteRequest(irp, IO_NO_INCREMENT as i8);
    STATUS_SUCCESS
}

/// IRP_MJ_CLOSE: always succeed.
pub unsafe extern "C" fn dispatch_close(
    _device_object: *mut DEVICE_OBJECT,
    irp: *mut IRP,
) -> NTSTATUS {
    (*irp).IoStatus.__bindgen_anon_1.Status = STATUS_SUCCESS;
    (*irp).IoStatus.Information = 0;
    IofCompleteRequest(irp, IO_NO_INCREMENT as i8);
    STATUS_SUCCESS
}

/// IRP_MJ_DEVICE_CONTROL: route based on IOCTL code.
pub unsafe extern "C" fn dispatch_device_control(
    device_object: *mut DEVICE_OBJECT,
    irp: *mut IRP,
) -> NTSTATUS {
    // Safely get stack location
    let stack_loc = match io_get_current_irp_stack_location(irp) {
        Ok(loc) => loc,
        Err(status) => {
            // Invalid IRP parameters
            (*irp).IoStatus.__bindgen_anon_1.Status = status;
            (*irp).IoStatus.Information = 0;
            IofCompleteRequest(irp, IO_NO_INCREMENT as i8);
            return status;
        }
    };

    let ioctl = (*stack_loc).Parameters.DeviceIoControl.IoControlCode;
    // Dispatch to specific handlers in callbacks module
    /*let status = match ioctl {
        IOCTL_GET_COUNTER => get_counter(device_object, irp),
        IOCTL_RESET_COUNTER => reset_counter(device_object, irp),
        _ => default_handler(device_object, irp),
    };
    */
    1 as NTSTATUS
}

// ------------------------------------------------------------
// Example registration:
//
// DISPATCHER.register(IRP_MJ_READ, dispatch_read);
// DISPATCHER.register(IRP_MJ_WRITE, dispatch_write);
// DISPATCHER.register(IRP_MJ_CREATE, dispatch_create);
// DISPATCHER.register(IRP_MJ_CLOSE, dispatch_close);
// DISPATCHER.register(IRP_MJ_DEVICE_CONTROL, dispatch_device_control);
