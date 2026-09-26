//! 更新の確認 (GitHub Releases の最新リリースとの比較)

use std::fmt;

use serde::Deserialize;

/// 最新リリースを返す GitHub API (下書きとプレリリースは含まれない)
pub const LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/Yasami/working-time-recorder/releases/latest";

/// 自動で更新できなかったときに開く、最新リリースのページ
pub const LATEST_RELEASE_PAGE: &str =
    "https://github.com/Yasami/working-time-recorder/releases/latest";

/// リリースの添付ファイルのうち、この名前で終わるものをインストーラーとみなす
const INSTALLER_SUFFIX: &str = "-setup.exe";

/// `x.y.z` 形式のバージョン
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    major: u64,
    minor: u64,
    patch: u64,
}

impl Version {
    /// `0.3.0` や `v0.3.0` を読む
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.strip_prefix('v').unwrap_or(text);
        let mut numbers = text.split('.').map(|part| {
            // u64::from_str は先頭の + を受け付けるので、数字だけかを先に調べる
            (!part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
                .then(|| part.parse().ok())
                .flatten()
        });
        match (
            numbers.next(),
            numbers.next(),
            numbers.next(),
            numbers.next(),
        ) {
            (Some(Some(major)), Some(Some(minor)), Some(Some(patch)), None) => Some(Self {
                major,
                minor,
                patch,
            }),
            _ => None,
        }
    }

    /// 実行中のビューワーのバージョン
    pub fn current() -> Self {
        Self::parse(env!("CARGO_PKG_VERSION"))
            .expect("パッケージのバージョンが x.y.z 形式ではありません")
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    pub version: Version,
    pub installer_url: String,
    /// インストーラーの SHA-256。GitHub が返さなければ None
    pub installer_sha256: Option<[u8; 32]>,
}

#[derive(Deserialize)]
struct RawRelease {
    tag_name: String,
    #[serde(default)]
    assets: Vec<RawAsset>,
}

#[derive(Deserialize)]
struct RawAsset {
    name: String,
    browser_download_url: String,
    /// `sha256:<16進数>` 形式
    digest: Option<String>,
}

/// GitHub API が返すリリースの JSON を読む
pub fn parse_release(json: &str) -> Result<Release, String> {
    let raw: RawRelease =
        serde_json::from_str(json).map_err(|e| format!("リリースの情報を読み取れません: {e}"))?;
    let version = Version::parse(&raw.tag_name)
        .ok_or_else(|| format!("リリースのバージョンが正しくありません: {}", raw.tag_name))?;
    let installer = raw
        .assets
        .into_iter()
        .find(|asset| asset.name.to_ascii_lowercase().ends_with(INSTALLER_SUFFIX))
        .ok_or_else(|| format!("リリース {} にインストーラーがありません", raw.tag_name))?;
    let installer_sha256 = match installer.digest.as_deref() {
        Some(digest) => parse_sha256_digest(digest)
            .map_err(|()| format!("インストーラーのハッシュ値が正しくありません: {digest}"))?,
        None => None,
    };
    Ok(Release {
        version,
        installer_url: installer.browser_download_url,
        installer_sha256,
    })
}

/// `sha256:<16進数>` を読む。SHA-256 以外のアルゴリズムなら None
fn parse_sha256_digest(digest: &str) -> Result<Option<[u8; 32]>, ()> {
    let Some(hex) = digest.strip_prefix("sha256:") else {
        return Ok(None);
    };
    // from_str_radix は先頭の + を受け付けるので、16 進数の数字だけかを先に調べる
    if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(());
    }
    let mut hash = [0; 32];
    for (i, byte) in hash.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).map_err(|_| ())?;
    }
    Ok(Some(hash))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(major: u64, minor: u64, patch: u64) -> Version {
        Version {
            major,
            minor,
            patch,
        }
    }

    #[test]
    fn test_parse_version() {
        assert_eq!(Version::parse("0.3.0"), Some(version(0, 3, 0)));
        assert_eq!(Version::parse("v1.20.300"), Some(version(1, 20, 300)));
        assert_eq!(Version::parse("1.2"), None);
        assert_eq!(Version::parse("1.2.3.4"), None);
        assert_eq!(Version::parse("1.2.3-beta"), None);
        assert_eq!(Version::parse("1.+2.3"), None);
        assert_eq!(Version::parse("1..3"), None);
        assert_eq!(Version::parse(""), None);
    }

    #[test]
    fn test_current_version() {
        assert_eq!(
            Version::current().to_string(),
            env!("CARGO_PKG_VERSION").to_string()
        );
    }

    #[test]
    fn test_version_order() {
        assert!(version(0, 10, 0) > version(0, 9, 9));
        assert!(version(1, 0, 0) > version(0, 99, 99));
        assert!(version(0, 2, 1) > version(0, 2, 0));
    }

    #[test]
    fn test_parse_release() {
        let release = parse_release(
            r#"{
                "tag_name": "v0.3.0",
                "draft": false,
                "assets": [
                    {
                        "name": "notes.txt",
                        "browser_download_url": "https://example.com/notes.txt",
                        "digest": null
                    },
                    {
                        "name": "working-time-recorder-v0.3.0-windows-x86_64-setup.exe",
                        "browser_download_url": "https://example.com/setup.exe",
                        "digest": "sha256:edb7cf5e05ecdf3a4094bbdf3a41587690198b8e9d37b0ee89c44379fbb3e50d"
                    }
                ]
            }"#,
        )
        .unwrap();
        assert_eq!(release.version, version(0, 3, 0));
        assert_eq!(release.installer_url, "https://example.com/setup.exe");
        let sha256 = release.installer_sha256.unwrap();
        assert_eq!(sha256[0], 0xED);
        assert_eq!(sha256[31], 0x0D);
    }

    #[test]
    fn test_parse_release_without_digest() {
        let release = parse_release(
            r#"{
                "tag_name": "v0.3.0",
                "assets": [
                    { "name": "A-SETUP.EXE", "browser_download_url": "https://example.com/a" }
                ]
            }"#,
        )
        .unwrap();
        assert_eq!(release.installer_sha256, None);
    }

    #[test]
    fn test_parse_release_errors() {
        // インストーラーが無い
        assert!(parse_release(r#"{ "tag_name": "v0.3.0", "assets": [] }"#).is_err());
        // バージョンが x.y.z でない
        assert!(
            parse_release(
                r#"{ "tag_name": "latest", "assets": [
                    { "name": "a-setup.exe", "browser_download_url": "https://example.com/a" }
                ] }"#
            )
            .is_err()
        );
        // JSON でない
        assert!(parse_release("Not Found").is_err());
    }

    #[test]
    fn test_parse_sha256_digest() {
        let hex = "00ff".repeat(16);
        let hash = parse_sha256_digest(&format!("sha256:{hex}"))
            .unwrap()
            .unwrap();
        assert_eq!(hash[0], 0x00);
        assert_eq!(hash[1], 0xFF);
        assert_eq!(parse_sha256_digest("sha512:abcd"), Ok(None));
        assert!(parse_sha256_digest("sha256:abcd").is_err());
        assert!(parse_sha256_digest(&format!("sha256:{}", "zz".repeat(32))).is_err());
        assert!(parse_sha256_digest(&format!("sha256:+f{}", "0".repeat(62))).is_err());
        // 2 バイト文字が混ざっていても panic しない
        assert!(parse_sha256_digest(&format!("sha256:{}あ", "0".repeat(61))).is_err());
    }
}
