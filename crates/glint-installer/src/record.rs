//! `install.record`: the metadata file inside each store entry.
//!
//! It is written into the staging directory **before** the entry is renamed
//! into the store, so every store entry is complete the moment it appears.
//! If the database is lost, the package index is rebuilt from these records
//! after re-hashing each entry's archive against its content address.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use crate::error::InstallError;

/// File name of the record inside a store entry.
pub(crate) const RECORD_NAME: &str = "install.record";
const HEADER: &str = "glint-install-record 1";

/// What a store entry contains, independent of the database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoreRecord {
    pub digest: String,
    pub kind: String,
    pub package_id: String,
    pub version: String,
    pub archive_size: u64,
    pub source_kind: String,
    pub source_locator: String,
    pub installed_at: i64,
}

impl StoreRecord {
    /// Write the record as a new file in `dir`, flushed to disk.
    pub(crate) fn write_new(&self, dir: &Path) -> Result<(), InstallError> {
        let path = dir.join(RECORD_NAME);
        let text = format!(
            "{HEADER}\ndigest={}\nkind={}\nid={}\nversion={}\narchive_size={}\nsource_kind={}\nsource_locator={}\ninstalled_at={}\n",
            escape(&self.digest),
            escape(&self.kind),
            escape(&self.package_id),
            escape(&self.version),
            self.archive_size,
            escape(&self.source_kind),
            escape(&self.source_locator),
            self.installed_at,
        );
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(InstallError::io("write install record", &path))?;
        file.write_all(text.as_bytes())
            .and_then(|()| file.sync_all())
            .map_err(InstallError::io("write install record", &path))?;
        drop(file);
        crate::fsutil::set_file_read_only(&path)
            .map_err(InstallError::io("seal install record", &path))
    }

    /// Read a record; `None` when missing or malformed.
    pub(crate) fn read(dir: &Path) -> Option<Self> {
        let text = fs::read_to_string(dir.join(RECORD_NAME)).ok()?;
        let mut lines = text.lines();
        if lines.next()? != HEADER {
            return None;
        }
        let mut get = std::collections::HashMap::new();
        for line in lines {
            let (key, value) = line.split_once('=')?;
            get.insert(key.to_owned(), unescape(value)?);
        }
        let mut take = |key: &str| get.remove(key);
        Some(Self {
            digest: take("digest")?,
            kind: take("kind")?,
            package_id: take("id")?,
            version: take("version")?,
            archive_size: take("archive_size")?.parse().ok()?,
            source_kind: take("source_kind")?,
            source_locator: take("source_locator")?,
            installed_at: take("installed_at")?.parse().ok()?,
        })
    }
}

fn escape(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace('\n', "%0A")
        .replace('\r', "%0D")
}

fn unescape(value: &str) -> Option<String> {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(i) = rest.find('%') {
        out.push_str(&rest[..i]);
        let code = rest.get(i + 1..i + 3)?;
        out.push(match code {
            "25" => '%',
            "0A" => '\n',
            "0D" => '\r',
            _ => return None,
        });
        rest = &rest[i + 3..];
    }
    out.push_str(rest);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escaping_round_trips() {
        for value in ["plain", "100%", "a\nb\rc", "%0A literal"] {
            assert_eq!(unescape(&escape(value)).as_deref(), Some(value));
        }
        assert_eq!(unescape("%zz"), None);
    }
}
