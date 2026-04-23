mod class_factory;
mod config;
mod registration;
pub mod text_service;

use std::path::PathBuf;
use std::sync::atomic::{AtomicIsize, Ordering};

use windows::Win32::Foundation::{BOOL, E_FAIL, HINSTANCE, S_OK};
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::System::SystemServices::DLL_PROCESS_ATTACH;

pub use registration::{
    current_registered_module, is_registered, register_text_service, try_resolve_module_path,
    unregister_text_service,
};

static DLL_MODULE: AtomicIsize = AtomicIsize::new(0);

#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllMain(
    instance: HINSTANCE,
    reason: u32,
    _reserved: *mut core::ffi::c_void,
) -> BOOL {
    if reason == DLL_PROCESS_ATTACH {
        DLL_MODULE.store(instance.0 as isize, Ordering::SeqCst);
    }
    true.into()
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllCanUnloadNow() -> windows::core::HRESULT {
    S_OK.into()
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllGetClassObject(
    rclsid: *const windows::core::GUID,
    riid: *const windows::core::GUID,
    ppv: *mut *mut core::ffi::c_void,
) -> windows::core::HRESULT {
    class_factory::dll_get_class_object(rclsid, riid, ppv)
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllRegisterServer() -> windows::core::HRESULT {
    match current_module_path().and_then(|path| register_text_service(&path)) {
        Ok(_) => S_OK.into(),
        Err(_) => E_FAIL.into(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllUnregisterServer() -> windows::core::HRESULT {
    match unregister_text_service() {
        Ok(_) => S_OK.into(),
        Err(_) => E_FAIL.into(),
    }
}

fn current_module_path() -> anyhow::Result<PathBuf> {
    let raw = DLL_MODULE.load(Ordering::SeqCst);
    if raw == 0 {
        anyhow::bail!("DLL module handle not initialized");
    }

    let mut buffer = [0u16; 260];
    let len = unsafe { GetModuleFileNameW(HINSTANCE(raw as _), &mut buffer) } as usize;
    if len == 0 {
        anyhow::bail!("GetModuleFileNameW returned 0");
    }

    Ok(PathBuf::from(String::from_utf16_lossy(&buffer[..len])))
}
