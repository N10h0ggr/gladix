/*!
 * kernel-driver/src/security.rs
 *
 * Helpers to build SECURITY_DESCRIPTORs for named kernel objects.
 *
 * Right now `build_world_sd()` returns a SD with a NULL-DACL (meaning
 * **Everyone** has full access).  In production you can swap it for
 * `build_world_plus_sid_sd(app_sid)` to give full access to both
 * Everyone and your service’s SID.
 */

#![no_std]

use core::{ptr, mem::size_of};

use wdk_sys::{ntddk::{
    ExAllocatePool2, ExFreePoolWithTag,
    RtlCreateSecurityDescriptor, RtlSetDaclSecurityDescriptor,
    RtlCreateAcl, RtlAddAccessAllowedAce,
    RtlAllocateAndInitializeSid, RtlFreeSid,
}, NTSTATUS, STATUS_INSUFFICIENT_RESOURCES, STATUS_SUCCESS, SECURITY_DESCRIPTOR, ACL, PSID, SECTION_ALL_ACCESS, POOL_FLAG_PAGED, SECURITY_DESCRIPTOR_REVISION, SID_IDENTIFIER_AUTHORITY, SECURITY_WORLD_RID, ACL_REVISION, PSECURITY_DESCRIPTOR, SIZE_T};

/// Pool tag 'SDSC'
const POOL_TAG: u32 = u32::from_le_bytes(*b"SDSC");

/// Mask for read/write/execute mapping on sections
/// (if you need EXECUTE, OR in SECTION_MAP_EXECUTE)
pub const SECTION_RWX_MASK: u32 = SECTION_ALL_ACCESS;

/// Build a security descriptor whose DACL is `NULL` – that allows
/// unprivileged user-mode processes to open the section for *any*
/// access they ask for.
///
/// # Safety
/// Caller must eventually free the returned pointer with
/// [`free_sd()`] once the section has been closed.
pub unsafe fn build_world_sd() -> Result<PSECURITY_DESCRIPTOR, NTSTATUS> {
    // Allocate SD using ExAllocatePool2
    let sd_ptr = ExAllocatePool2(
        POOL_FLAG_PAGED,
        size_of::<SECURITY_DESCRIPTOR>() as _,
        POOL_TAG,
    ) as PSECURITY_DESCRIPTOR;

    if sd_ptr.is_null() {
        return Err(STATUS_INSUFFICIENT_RESOURCES);
    }

    // Initialize descriptor
    let status = RtlCreateSecurityDescriptor(
        sd_ptr,
        SECURITY_DESCRIPTOR_REVISION as u32,
    );
    if status != STATUS_SUCCESS {
        ExFreePoolWithTag(sd_ptr.cast(), POOL_TAG);
        return Err(status);
    }

    // NULL-DACL → Everyone ALL_ACCESS
    let status = RtlSetDaclSecurityDescriptor(
        sd_ptr,
        1,              // DaclPresent = TRUE
        ptr::null_mut(),// pDacl       = NULL
        0,              // DaclDefaulted = FALSE
    );
    if status != STATUS_SUCCESS {
        ExFreePoolWithTag(sd_ptr.cast(), POOL_TAG);
        return Err(status);
    }

    Ok(sd_ptr)
}

/// Future-proof helper: build a DACL that grants
///   • Everyone      – full access
///   • `extra_sid`   – full access (if provided)
///
/// When `extra_sid` is `None` the result is identical to
/// [`build_world_sd()`].
///
/// # Safety
/// Same lifetime rules as [`build_world_sd()`] apply.
pub unsafe fn build_world_plus_sid_sd(
    extra_sid: Option<PSID>,
) -> Result<PSECURITY_DESCRIPTOR, NTSTATUS> {
    // Worst-case: 2 ACEs + ACL + SD
    let acl_len = size_of::<ACL>()
        + 2 * (size_of::<wdk_sys::ACCESS_ALLOWED_ACE>() + size_of::<u32>());

    let total = size_of::<SECURITY_DESCRIPTOR>() + acl_len;
    let block = ExAllocatePool2(
        POOL_FLAG_PAGED,
        total as SIZE_T,
        POOL_TAG,
    ) as *mut u8;
    if block.is_null() {
        return Err(STATUS_INSUFFICIENT_RESOURCES);
    }
    ptr::write_bytes(block, 0, total);

    let sd = block as PSECURITY_DESCRIPTOR;
    let acl = block.add(size_of::<SECURITY_DESCRIPTOR>()) as *mut ACL;

    // Initialize SD
    let mut status = RtlCreateSecurityDescriptor(
        sd,
        SECURITY_DESCRIPTOR_REVISION as u32,
    );
    if status != STATUS_SUCCESS {
        ExFreePoolWithTag(block.cast(), POOL_TAG);
        return Err(status);
    }

    // Initialize ACL
    // Create ACL using RtlCreateAcl
    if RtlCreateAcl(acl, acl_len as u32, ACL_REVISION as u32) != STATUS_SUCCESS {
        ExFreePoolWithTag(block.cast(), POOL_TAG);
        return Err(STATUS_INSUFFICIENT_RESOURCES);
    }

    // Everyone SID
    let mut world_auth = SID_IDENTIFIER_AUTHORITY { Value: [0,0,0,0,0,1] };
    let mut world_sid: PSID = ptr::null_mut();
    status = RtlAllocateAndInitializeSid(
        &mut world_auth,
        1,
        SECURITY_WORLD_RID as u32,
        0,0,0,0,0,0,0,
        &mut world_sid,
    );
    if status != STATUS_SUCCESS {
        ExFreePoolWithTag(block.cast(), POOL_TAG);
        return Err(status);
    }

    // ACE 1: Everyone
    if RtlAddAccessAllowedAce(acl, ACL_REVISION as u32, SECTION_ALL_ACCESS, world_sid) == 0 {
        RtlFreeSid(world_sid);
        ExFreePoolWithTag(block.cast(), POOL_TAG);
        return Err(STATUS_INSUFFICIENT_RESOURCES);
    }

    // ACE 2: extra SID
    if let Some(sid) = extra_sid {
        if RtlAddAccessAllowedAce(acl, ACL_REVISION as u32, SECTION_ALL_ACCESS, sid) == 0 {
            RtlFreeSid(world_sid);
            ExFreePoolWithTag(block.cast(), POOL_TAG);
            return Err(STATUS_INSUFFICIENT_RESOURCES);
        }
    }

    // Attach DACL
    status = RtlSetDaclSecurityDescriptor(sd, 1, acl, 0);
    if status != STATUS_SUCCESS {
        RtlFreeSid(world_sid);
        ExFreePoolWithTag(block.cast(), POOL_TAG);
        return Err(status);
    }

    // Cleanup
    RtlFreeSid(world_sid);

    Ok(sd)
}

/// Free a security descriptor created by this module.
pub unsafe fn free_sd(sd: *mut SECURITY_DESCRIPTOR) {
    if !sd.is_null() {
        ExFreePoolWithTag(sd.cast(), POOL_TAG);
    }
}
