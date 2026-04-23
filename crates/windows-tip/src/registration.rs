use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::UI::TextServices::{
    CLSID_TF_CategoryMgr, CLSID_TF_InputProcessorProfiles, GUID_TFCAT_TIP_KEYBOARD, ITfCategoryMgr,
    ITfInputProcessorProfiles,
};
use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};

use crate::config::{
    CLSID_TEXT_SERVICE, GUID_LANGUAGE_PROFILE, ICON_INDEX, LANGID_NEPALI_NEPAL,
    LANGUAGE_PROFILE_NAME, TEXT_SERVICE_NAME,
};

fn clsid_string() -> String {
    format!("{CLSID_TEXT_SERVICE:?}")
}

fn hkcu_classes_root() -> Result<RegKey> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    hkcu.create_subkey("Software\\Classes")
        .map(|(key, _)| key)
        .context("failed to open HKCU\\Software\\Classes")
}

pub fn register_text_service(module_path: &Path) -> Result<()> {
    write_com_registration(module_path)?;
    register_with_tsf(module_path)?;
    Ok(())
}

pub fn unregister_text_service() -> Result<()> {
    unregister_from_tsf()?;

    let classes = hkcu_classes_root()?;
    let _ = classes.delete_subkey_all(format!("CLSID\\{}", clsid_string()));
    Ok(())
}

pub fn is_registered() -> Result<bool> {
    let classes = hkcu_classes_root()?;
    Ok(classes
        .open_subkey_with_flags(
            format!("CLSID\\{}\\InprocServer32", clsid_string()),
            KEY_READ,
        )
        .is_ok())
}

pub fn current_registered_module() -> Result<Option<PathBuf>> {
    let classes = hkcu_classes_root()?;
    let key = match classes.open_subkey_with_flags(
        format!("CLSID\\{}\\InprocServer32", clsid_string()),
        KEY_READ,
    ) {
        Ok(key) => key,
        Err(_) => return Ok(None),
    };
    let value: String = key
        .get_value("")
        .context("failed to read InprocServer32 default value")?;
    Ok(Some(PathBuf::from(value)))
}

fn write_com_registration(module_path: &Path) -> Result<()> {
    let classes = hkcu_classes_root()?;
    let clsid_key_path = format!("CLSID\\{}", clsid_string());
    let (clsid_key, _) = classes
        .create_subkey(&clsid_key_path)
        .with_context(|| format!("failed to create HKCU\\Software\\Classes\\{clsid_key_path}"))?;
    clsid_key
        .set_value("", &TEXT_SERVICE_NAME)
        .context("failed to set CLSID display name")?;

    let (inproc_key, _) = clsid_key
        .create_subkey("InprocServer32")
        .context("failed to create InprocServer32 key")?;
    inproc_key
        .set_value("", &module_path.display().to_string())
        .context("failed to set InprocServer32 path")?;
    inproc_key
        .set_value("ThreadingModel", &"Apartment")
        .context("failed to set COM threading model")?;

    Ok(())
}

fn register_with_tsf(module_path: &Path) -> Result<()> {
    unsafe {
        let profiles: ITfInputProcessorProfiles =
            CoCreateInstance(&CLSID_TF_InputProcessorProfiles, None, CLSCTX_INPROC_SERVER)
                .context("failed to create ITfInputProcessorProfiles")?;

        if let Err(e) = profiles.Register(&CLSID_TEXT_SERVICE) {
            anyhow::bail!(
                "ITfInputProcessorProfiles::Register failed for CLSID {} (error: {})",
                clsid_string(),
                e
            );
        }

        profiles
            .AddLanguageProfile(
                &CLSID_TEXT_SERVICE,
                LANGID_NEPALI_NEPAL,
                &GUID_LANGUAGE_PROFILE,
                &to_wide(LANGUAGE_PROFILE_NAME),
                &to_wide(&module_path.display().to_string()),
                ICON_INDEX,
            )
            .ok()
            .context("ITfInputProcessorProfiles::AddLanguageProfile failed")?;

        let category_mgr: ITfCategoryMgr =
            CoCreateInstance(&CLSID_TF_CategoryMgr, None, CLSCTX_INPROC_SERVER)
                .context("failed to create ITfCategoryMgr")?;

        category_mgr
            .RegisterCategory(
                &CLSID_TEXT_SERVICE,
                &GUID_TFCAT_TIP_KEYBOARD,
                &CLSID_TEXT_SERVICE,
            )
            .ok()
            .context("ITfCategoryMgr::RegisterCategory failed")?;
    }

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let path = format!(
        "Software\\Microsoft\\CTF\\TIP\\{}\\LanguageProfile\\0x{:04x}\\{}",
        clsid_string(),
        LANGID_NEPALI_NEPAL,
        format!("{GUID_LANGUAGE_PROFILE:?}")
    );
    let (key, _) = hkcu
        .create_subkey(&path)
        .with_context(|| format!("failed to create HKCU\\{path}"))?;
    key.set_value("Description", &LANGUAGE_PROFILE_NAME)
        .context("failed to set profile description")?;
    key.set_value("Display Description", &TEXT_SERVICE_NAME)
        .context("failed to set display description")?;
    key.set_value("Enable", &1u32)
        .context("failed to enable profile")?;

    Ok(())
}

fn to_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

fn unregister_from_tsf() -> Result<()> {
    unsafe {
        let profiles: ITfInputProcessorProfiles =
            CoCreateInstance(&CLSID_TF_InputProcessorProfiles, None, CLSCTX_INPROC_SERVER)
                .context("failed to create ITfInputProcessorProfiles")?;
        let _ = profiles.Unregister(&CLSID_TEXT_SERVICE).ok();

        let category_mgr: ITfCategoryMgr =
            CoCreateInstance(&CLSID_TF_CategoryMgr, None, CLSCTX_INPROC_SERVER)
                .context("failed to create ITfCategoryMgr")?;
        let _ = category_mgr
            .UnregisterCategory(
                &CLSID_TEXT_SERVICE,
                &GUID_TFCAT_TIP_KEYBOARD,
                &CLSID_TEXT_SERVICE,
            )
            .ok();
    }

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let path = format!("Software\\Microsoft\\CTF\\TIP\\{}", clsid_string());
    let _ = hkcu.delete_subkey_all(path);
    Ok(())
}

pub fn try_resolve_module_path(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path.to_path_buf());
    }

    if let Some(path) = current_registered_module()? {
        return Ok(path);
    }

    let mut path = std::env::current_exe().context("failed to locate current executable")?;
    path.pop();
    path.push("windows_tip.dll");
    if !path.exists() {
        path.set_file_name("windows_tip.dll");
    }
    Ok(path)
}
