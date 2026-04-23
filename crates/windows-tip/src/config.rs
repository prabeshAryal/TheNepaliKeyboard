use windows::core::GUID;

pub const TEXT_SERVICE_NAME: &str = "The Nepali Keyboard";
pub const LANGUAGE_PROFILE_NAME: &str = "Nepali Transliteration";
pub const ICON_INDEX: u32 = 0;
pub const LANGID_NEPALI_NEPAL: u16 = 0x0461;

pub const CLSID_TEXT_SERVICE: GUID = GUID::from_u128(0x7dbfdb70_87e0_4db9_81f0_0e4a5f4b7b71);
pub const GUID_LANGUAGE_PROFILE: GUID = GUID::from_u128(0x5ea5fc43_90e6_4d0f_8c59_f85789d4be3a);
