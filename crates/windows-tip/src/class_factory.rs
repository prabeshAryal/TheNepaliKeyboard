//! COM Class Factory that creates `NepaliTextService` instances.
//!
//! `DllGetClassObject` returns an instance of this factory. The TSF
//! framework then calls `IClassFactory::CreateInstance` to obtain the
//! `ITfTextInputProcessor` that drives the keyboard.

use std::ffi::c_void;

use windows::core::{implement, IUnknown, Interface, Result, GUID};
use windows::Win32::Foundation::{BOOL, CLASS_E_CLASSNOTAVAILABLE, E_POINTER, S_OK};
use windows::Win32::System::Com::{IClassFactory, IClassFactory_Impl};
use windows::Win32::UI::TextServices::ITfTextInputProcessor;

use crate::config::CLSID_TEXT_SERVICE;
use crate::text_service::NepaliTextService;

#[implement(IClassFactory)]
pub struct NepaliClassFactory;

impl IClassFactory_Impl for NepaliClassFactory_Impl {
    fn CreateInstance(
        &self,
        _punkouter: Option<&IUnknown>,
        riid: *const GUID,
        ppvobject: *mut *mut c_void,
    ) -> Result<()> {
        if ppvobject.is_null() {
            return Err(E_POINTER.into());
        }

        unsafe {
            *ppvobject = core::ptr::null_mut();
        }

        // Create a new text-service instance.
        let service = NepaliTextService::new();
        let tip: ITfTextInputProcessor = service.into();

        // QueryInterface for the requested IID.
        unsafe {
            let hr = tip.query(&*riid, ppvobject);
            hr.ok()?;
        }
        Ok(())
    }

    fn LockServer(&self, _flock: BOOL) -> Result<()> {
        Ok(())
    }
}

/// The real DllGetClassObject implementation, called from `lib.rs`.
pub fn dll_get_class_object(
    rclsid: *const GUID,
    riid: *const GUID,
    ppv: *mut *mut c_void,
) -> windows::core::HRESULT {
    if rclsid.is_null() || riid.is_null() || ppv.is_null() {
        return E_POINTER.into();
    }

    unsafe {
        *ppv = core::ptr::null_mut();

        if *rclsid != CLSID_TEXT_SERVICE {
            return CLASS_E_CLASSNOTAVAILABLE.into();
        }

        let factory = NepaliClassFactory;
        let factory_iface: IClassFactory = factory.into();

        let hr = factory_iface.query(&*riid, ppv);
        if hr.is_ok() {
            S_OK.into()
        } else {
            hr
        }
    }
}
