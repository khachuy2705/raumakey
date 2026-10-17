//! Side-by-side application manifest builder.
//!
//! Every manifest produced here requests `uiAccess="false"`, depends on Common Controls v6,
//! declares Windows 10/11 support, Per-Monitor V2 DPI awareness (with Per-Monitor fallbacks),
//! and the UTF-8 active code page.
//!
//! The UTF-8 active code page makes `GetACP()` and `CP_ACP` mean 65001 inside the executable.
//! Code that converts legacy 8-bit text must therefore name its code page (for example 1252 or
//! 1258) instead of relying on `CP_ACP`.

use crate::error::ResourceError;
use crate::version::dotted;

/// Resource name of an executable's manifest (`CREATEPROCESS_MANIFEST_RESOURCE_ID`).
pub const MANIFEST_ID: u16 = 1;

/// `supportedOS` id that covers Windows 10 and Windows 11.
pub const SUPPORTED_OS_WINDOWS_10: &str = "{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}";

const CONTROLLER_IDENTITY: &str = "Raumakey.Controller";
const CONTROLLER_DESCRIPTION: &str = "Raumakey Vietnamese input method";
const MAX_IDENTITY_LEN: usize = 256;

/// Requested execution level (`requestedExecutionLevel/@level`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExecutionLevel {
    /// Runs with the caller's token (`asInvoker`). This is the Raumakey default.
    AsInvoker,
    /// Elevates when the user is an administrator (`highestAvailable`).
    HighestAvailable,
    /// Always requires elevation (`requireAdministrator`).
    RequireAdministrator,
}

impl ExecutionLevel {
    const fn as_str(self) -> &'static str {
        match self {
            Self::AsInvoker => "asInvoker",
            Self::HighestAvailable => "highestAvailable",
            Self::RequireAdministrator => "requireAdministrator",
        }
    }
}

/// An executable's application manifest (`RT_MANIFEST` resource data).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    identity_name: String,
    description: String,
    version: [u16; 4],
    execution_level: ExecutionLevel,
}

impl Manifest {
    /// Manifest of the Raumakey controller (`raumakey.exe`), running `asInvoker`.
    pub fn controller(version: [u16; 4]) -> Self {
        Self {
            identity_name: CONTROLLER_IDENTITY.to_owned(),
            description: CONTROLLER_DESCRIPTION.to_owned(),
            version,
            execution_level: ExecutionLevel::AsInvoker,
        }
    }

    /// Manifest of another executable, running `asInvoker`.
    ///
    /// `identity_name` is 1 to 256 ASCII letters, digits, `.`, `-`, or `_`. `description` may
    /// not contain control characters (U+0000 to U+001F and U+007F to U+009F) other than tab,
    /// line feed, and carriage return, nor the noncharacters U+FFFE and U+FFFF, which XML 1.0
    /// does not allow. Otherwise fails with [`ResourceError::InvalidText`].
    pub fn executable(
        identity_name: &str,
        description: &str,
        version: [u16; 4],
    ) -> Result<Self, ResourceError> {
        let valid_name = !identity_name.is_empty()
            && identity_name.len() <= MAX_IDENTITY_LEN
            && identity_name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'));
        let valid_description = description.chars().all(|c| {
            (!c.is_control() || matches!(c, '\t' | '\n' | '\r'))
                && !matches!(c, '\u{FFFE}' | '\u{FFFF}')
        });
        if !valid_name || !valid_description {
            return Err(ResourceError::InvalidText);
        }
        Ok(Self {
            identity_name: identity_name.to_owned(),
            description: description.to_owned(),
            version,
            execution_level: ExecutionLevel::AsInvoker,
        })
    }

    /// Returns the manifest with another execution level. `uiAccess` stays `false`.
    pub fn with_execution_level(mut self, level: ExecutionLevel) -> Self {
        self.execution_level = level;
        self
    }

    /// Assembly identity name.
    pub fn identity_name(&self) -> &str {
        &self.identity_name
    }

    /// Human-readable description.
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Assembly identity version.
    pub fn version(&self) -> [u16; 4] {
        self.version
    }

    /// Requested execution level.
    pub fn execution_level(&self) -> ExecutionLevel {
        self.execution_level
    }

    /// Serializes the manifest as UTF-8 XML without a byte-order mark.
    pub fn to_xml(&self) -> String {
        let lines = [
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#.to_owned(),
            r#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">"#
                .to_owned(),
            format!(
                r#"  <assemblyIdentity type="win32" name="{}" version="{}"/>"#,
                escape(&self.identity_name),
                dotted(self.version)
            ),
            format!(
                "  <description>{}</description>",
                escape(&self.description)
            ),
            "  <dependency>".to_owned(),
            "    <dependentAssembly>".to_owned(),
            concat!(
                r#"      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls""#,
                r#" version="6.0.0.0" processorArchitecture="*""#,
                r#" publicKeyToken="6595b64144ccf1df" language="*"/>"#
            )
            .to_owned(),
            "    </dependentAssembly>".to_owned(),
            "  </dependency>".to_owned(),
            r#"  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">"#.to_owned(),
            "    <security>".to_owned(),
            "      <requestedPrivileges>".to_owned(),
            format!(
                r#"        <requestedExecutionLevel level="{}" uiAccess="false"/>"#,
                self.execution_level.as_str()
            ),
            "      </requestedPrivileges>".to_owned(),
            "    </security>".to_owned(),
            "  </trustInfo>".to_owned(),
            r#"  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">"#.to_owned(),
            "    <application>".to_owned(),
            format!(r#"      <supportedOS Id="{SUPPORTED_OS_WINDOWS_10}"/>"#),
            "    </application>".to_owned(),
            "  </compatibility>".to_owned(),
            r#"  <application xmlns="urn:schemas-microsoft-com:asm.v3">"#.to_owned(),
            "    <windowsSettings>".to_owned(),
            concat!(
                r#"      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">"#,
                "true/pm</dpiAware>"
            )
            .to_owned(),
            concat!(
                r#"      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">"#,
                "PerMonitorV2, PerMonitor</dpiAwareness>"
            )
            .to_owned(),
            concat!(
                r#"      <activeCodePage xmlns="http://schemas.microsoft.com/SMI/2019/WindowsSettings">"#,
                "UTF-8</activeCodePage>"
            )
            .to_owned(),
            "    </windowsSettings>".to_owned(),
            "  </application>".to_owned(),
            "</assembly>".to_owned(),
        ];
        let mut xml = lines.join("\n");
        xml.push('\n');
        xml
    }
}

/// Escapes text for XML character data and quoted attribute values.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            other => out.push(other),
        }
    }
    out
}
