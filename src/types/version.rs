#[cfg(not(feature = "std"))]
use alloc::string::String;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use serde::Deserialize;

fn default_visible() -> bool {
    true
}

/// Hardware or firmware module entry from the printer's expansion bus version database.
#[derive(Debug, Clone, Deserialize)]
pub struct VersionModule {
    #[serde(default)]
    pub product_name: String,
    pub name: String,
    #[serde(default)]
    pub hw_ver: String,
    #[serde(default)]
    pub sw_ver: String,
    #[serde(default)]
    pub sn: String,
    #[serde(default = "default_visible")]
    pub visible: bool,
}

/// Typed response from a `get_version` command containing all expansion bus modules.
#[derive(Debug, Clone, Deserialize)]
pub struct VersionInfo {
    pub command: String,
    pub sequence_id: String,
    #[serde(default)]
    pub module: Vec<VersionModule>,
}

/// Wire-level JSON wrapper for the `get_version` response.
#[derive(Debug, Clone, Deserialize)]
#[cfg(test)]
pub(crate) struct GetVersionResponse {
    pub info: VersionInfo,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_info_deserialization() {
        let json = r#"{
            "info": {
                "command": "get_version",
                "sequence_id": "10002",
                "module": [
                    {
                        "product_name": "Bambu Lab X1 Carbon",
                        "name": "ota",
                        "hw_ver": "OTA",
                        "sw_ver": "01.09.00.00",
                        "sn": "00M000000000001",
                        "visible": true
                    },
                    {
                        "name": "esp32",
                        "sw_ver": "01.02.03.04",
                        "sn": "00M000000000002"
                    }
                ]
            }
        }"#;

        let resp: GetVersionResponse = serde_json::from_str(json).unwrap();
        let info = resp.info;

        assert_eq!(info.command, "get_version");
        assert_eq!(info.sequence_id, "10002");
        assert_eq!(info.module.len(), 2);

        assert_eq!(info.module[0].product_name, "Bambu Lab X1 Carbon");
        assert_eq!(info.module[0].name, "ota");
        assert!(info.module[0].visible);

        // Second module uses defaults for missing fields
        assert_eq!(info.module[1].product_name, "");
        assert_eq!(info.module[1].hw_ver, "");
        assert!(info.module[1].visible, "visible should default to true");
    }

    #[test]
    fn test_version_info_empty_modules() {
        let json = r#"{
            "command": "get_version",
            "sequence_id": "10002"
        }"#;

        let info: VersionInfo = serde_json::from_str(json).unwrap();
        assert!(info.module.is_empty());
    }

    #[test]
    fn test_version_module_visible_false() {
        let json = r#"{
            "name": "internal",
            "sw_ver": "1.0.0",
            "visible": false
        }"#;

        let module: VersionModule = serde_json::from_str(json).unwrap();
        assert!(!module.visible);
    }
}
