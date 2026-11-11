//! Which festival skin MausBot is wearing, read without touching its data.
//!
//! MausBot keeps its skin (`omb-skin`) and the "Dress my bots up" switch
//! (`omb-halloween-dress`) in the Local Storage of its Electron window, which is a
//! LevelDB. The app holds the database open, so this reads the files directly and
//! read-only: the write-ahead log and the tables, newest write wins. It never takes
//! the `LOCK`, never copies, never writes.

use std::fs;
use std::path::{Path, PathBuf};

use crate::paths;

const SKIN_KEY: &str = "omb-skin";
const DRESS_KEY: &str = "omb-halloween-dress";
const FILE_LIMIT: u64 = 16 << 20;
const BLOCK_LIMIT: usize = 4 << 20;

const FORCE_SKIN: &str = "MISSION_SKIN";
const FORCE_DRESS: &str = "MISSION_DRESS";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Setting {
    pub skin: Option<String>,
    pub dress: Option<String>,
}

impl Setting {
    pub fn dressed(&self) -> bool {
        self.dress.as_deref() != Some("0")
    }

    pub fn festival(&self) -> Option<String> {
        let skin = self.skin.as_deref().filter(|skin| crate::mascot::festival_skin(skin))?;
        self.dressed().then(|| skin.to_string())
    }
}

pub fn directory() -> Option<PathBuf> {
    match std::env::var_os("OPENMAUSBOT_LOCAL_STORAGE") {
        Some(value) if !value.is_empty() => Some(PathBuf::from(value)),
        _ => paths::config_home().map(|config| config.join("openmausbot").join("Local Storage").join("leveldb")),
    }
}

pub fn forced() -> Option<Setting> {
    let skin = std::env::var(FORCE_SKIN).ok().filter(|value| !value.is_empty())?;
    Some(Setting { skin: Some(skin), dress: std::env::var(FORCE_DRESS).ok().filter(|value| !value.is_empty()) })
}

pub fn stamps(directory: &Path) -> Vec<(String, u64, i64, i64)> {
    use std::os::unix::fs::MetadataExt;
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut stamps: Vec<_> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            if !(name.ends_with(".log") || name.ends_with(".ldb")) {
                return None;
            }
            let metadata = entry.metadata().ok()?;
            Some((name, metadata.len(), metadata.mtime(), metadata.mtime_nsec()))
        })
        .collect();
    stamps.sort();
    stamps
}

pub fn read(directory: &Path) -> Option<Setting> {
    let entries = fs::read_dir(directory).ok()?;
    let mut skin = Newest::default();
    let mut dress = Newest::default();
    let mut found = false;
    for entry in entries.flatten() {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        let path = entry.path();
        let Some(bytes) = read_small(&path) else {
            continue;
        };
        found = true;
        let mut offer = |key: &[u8], seq: u64, value: Option<&[u8]>| {
            if let Some(name) = storage_key(key) {
                if name == SKIN_KEY.as_bytes() {
                    skin.offer(seq, value);
                } else if name == DRESS_KEY.as_bytes() {
                    dress.offer(seq, value);
                }
            }
        };
        if name.ends_with(".log") {
            scan_log(&bytes, &mut offer);
        } else if name.ends_with(".ldb") {
            scan_table(&bytes, &mut offer);
        }
    }
    found.then(|| Setting { skin: skin.text(), dress: dress.text() })
}

fn read_small(path: &Path) -> Option<Vec<u8>> {
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    let file = fs::OpenOptions::new().read(true).custom_flags(paths::READ_FLAGS).open(path).ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > FILE_LIMIT {
        return None;
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(FILE_LIMIT).read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

#[derive(Default)]
struct Newest {
    seq: Option<u64>,
    value: Option<Vec<u8>>,
}

impl Newest {
    fn offer(&mut self, seq: u64, value: Option<&[u8]>) {
        if self.seq.is_some_and(|held| held >= seq) {
            return;
        }
        self.seq = Some(seq);
        self.value = value.map(<[u8]>::to_vec);
    }

    fn text(&self) -> Option<String> {
        let raw = self.value.as_deref()?;
        let (flag, body) = raw.split_first()?;
        let text = match flag {
            1 => body.iter().map(|byte| *byte as char).collect::<String>(),
            0 => {
                let units: Vec<u16> = body.chunks_exact(2).map(|pair| u16::from_le_bytes([pair[0], pair[1]])).collect();
                String::from_utf16(&units).ok()?
            }
            _ => return None,
        };
        let text = text.trim().to_string();
        (!text.is_empty() && text.len() <= 64).then_some(text)
    }
}

fn storage_key(key: &[u8]) -> Option<&[u8]> {
    let rest = key.strip_prefix(b"_")?;
    let split = rest.iter().position(|byte| *byte == 0)?;
    let (origin, name) = (&rest[..split], &rest[split + 1..]);
    let local = origin.starts_with(b"http://127.0.0.1:") || origin.starts_with(b"http://localhost:");
    if !local {
        return None;
    }
    name.strip_prefix(&[1u8])
}

fn varint(bytes: &[u8], at: &mut usize) -> Option<u64> {
    let mut value = 0u64;
    for shift in (0..64).step_by(7) {
        let byte = *bytes.get(*at)?;
        *at += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

fn slice<'a>(bytes: &'a [u8], at: &mut usize, length: u64) -> Option<&'a [u8]> {
    let end = at.checked_add(usize::try_from(length).ok()?)?;
    let found = bytes.get(*at..end)?;
    *at = end;
    Some(found)
}

fn scan_log(bytes: &[u8], offer: &mut dyn FnMut(&[u8], u64, Option<&[u8]>)) {
    const BLOCK: usize = 32768;
    let mut batch: Vec<u8> = Vec::new();
    let mut at = 0;
    while at + 7 <= bytes.len() {
        let in_block = at % BLOCK;
        if BLOCK - in_block < 7 {
            at += BLOCK - in_block;
            continue;
        }
        let length = usize::from(u16::from_le_bytes([bytes[at + 4], bytes[at + 5]]));
        let kind = bytes[at + 6];
        let Some(payload) = bytes.get(at + 7..at + 7 + length) else {
            return;
        };
        at += 7 + length;
        match kind {
            1 => {
                scan_batch(payload, offer);
                batch.clear();
            }
            2 => {
                batch.clear();
                batch.extend_from_slice(payload);
            }
            3 => batch.extend_from_slice(payload),
            4 => {
                batch.extend_from_slice(payload);
                scan_batch(&batch, offer);
                batch.clear();
            }
            _ => {
                if kind != 0 {
                    return;
                }
            }
        }
    }
}

fn scan_batch(batch: &[u8], offer: &mut dyn FnMut(&[u8], u64, Option<&[u8]>)) {
    let Some(head) = batch.get(..12) else {
        return;
    };
    let base = u64::from_le_bytes(head[..8].try_into().unwrap_or_default());
    let count = u32::from_le_bytes(head[8..12].try_into().unwrap_or_default());
    let mut at = 12;
    for index in 0..u64::from(count) {
        let Some(kind) = batch.get(at).copied() else {
            return;
        };
        at += 1;
        let Some(key) = varint(batch, &mut at).and_then(|length| slice(batch, &mut at, length)) else {
            return;
        };
        match kind {
            1 => {
                let Some(value) = varint(batch, &mut at).and_then(|length| slice(batch, &mut at, length)) else {
                    return;
                };
                offer(key, base + index, Some(value));
            }
            0 => offer(key, base + index, None),
            _ => return,
        }
    }
}

const TABLE_MAGIC: [u8; 8] = [0x57, 0xfb, 0x80, 0x8b, 0x24, 0x75, 0x47, 0xdb];

fn scan_table(bytes: &[u8], offer: &mut dyn FnMut(&[u8], u64, Option<&[u8]>)) {
    let Some(footer) = bytes.len().checked_sub(48).and_then(|start| bytes.get(start..)) else {
        return;
    };
    if footer[40..] != TABLE_MAGIC {
        return;
    }
    let mut at = 0;
    let handle = |at: &mut usize| Some((varint(footer, at)?, varint(footer, at)?));
    if handle(&mut at).is_none() {
        return;
    }
    let Some((offset, size)) = handle(&mut at) else {
        return;
    };
    let Some(index) = block(bytes, offset, size) else {
        return;
    };
    for (_, handle) in entries(&index) {
        let mut at = 0;
        let (Some(offset), Some(size)) = (varint(&handle, &mut at), varint(&handle, &mut at)) else {
            continue;
        };
        let Some(data) = block(bytes, offset, size) else {
            continue;
        };
        for (internal, value) in entries(&data) {
            let Some(split) = internal.len().checked_sub(8) else {
                continue;
            };
            let tag = u64::from_le_bytes(internal[split..].try_into().unwrap_or_default());
            let live = tag & 0xff == 1;
            offer(&internal[..split], tag >> 8, live.then_some(value.as_slice()));
        }
    }
}

fn block(bytes: &[u8], offset: u64, size: u64) -> Option<Vec<u8>> {
    let start = usize::try_from(offset).ok()?;
    let length = usize::try_from(size).ok()?;
    let body = bytes.get(start..start.checked_add(length)?)?;
    match *bytes.get(start + length)? {
        0 => Some(body.to_vec()),
        1 => snappy(body),
        _ => None,
    }
}

fn entries(block: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
    let mut found = Vec::new();
    let Some(restarts) = block.len().checked_sub(4).and_then(|at| block.get(at..)) else {
        return found;
    };
    let count = u32::from_le_bytes(restarts.try_into().unwrap_or_default()) as usize;
    let Some(end) = count.checked_mul(4).and_then(|size| block.len().checked_sub(4 + size)) else {
        return found;
    };
    let mut at = 0;
    let mut key: Vec<u8> = Vec::new();
    while at < end {
        let (Some(shared), Some(fresh), Some(length)) =
            (varint(block, &mut at), varint(block, &mut at), varint(block, &mut at))
        else {
            break;
        };
        let Ok(shared) = usize::try_from(shared) else {
            break;
        };
        if shared > key.len() {
            break;
        }
        let Some(delta) = slice(&block[..end], &mut at, fresh) else {
            break;
        };
        let Some(value) = slice(&block[..end], &mut at, length) else {
            break;
        };
        key.truncate(shared);
        key.extend_from_slice(delta);
        found.push((key.clone(), value.to_vec()));
    }
    found
}

fn snappy(input: &[u8]) -> Option<Vec<u8>> {
    let mut at = 0;
    let total = usize::try_from(varint(input, &mut at)?).ok().filter(|total| *total <= BLOCK_LIMIT)?;
    let mut out: Vec<u8> = Vec::with_capacity(total);
    while at < input.len() {
        let tag = input[at];
        at += 1;
        match tag & 3 {
            0 => {
                let mut length = usize::from(tag >> 2);
                if length >= 60 {
                    let extra = length - 59;
                    let raw = input.get(at..at + extra)?;
                    at += extra;
                    length = raw.iter().rev().fold(0usize, |value, byte| (value << 8) | usize::from(*byte));
                }
                let literal = input.get(at..at.checked_add(length.checked_add(1)?)?)?;
                at += literal.len();
                out.extend_from_slice(literal);
            }
            kind => {
                let (length, offset) = match kind {
                    1 => {
                        let low = *input.get(at)?;
                        at += 1;
                        (4 + usize::from((tag >> 2) & 7), (usize::from(tag >> 5) << 8) | usize::from(low))
                    }
                    2 => {
                        let raw = input.get(at..at + 2)?;
                        at += 2;
                        (1 + usize::from(tag >> 2), usize::from(u16::from_le_bytes([raw[0], raw[1]])))
                    }
                    _ => {
                        let raw = input.get(at..at + 4)?;
                        at += 4;
                        (1 + usize::from(tag >> 2), u32::from_le_bytes(raw.try_into().ok()?) as usize)
                    }
                };
                if offset == 0 || offset > out.len() || out.len() + length > total {
                    return None;
                }
                for _ in 0..length {
                    out.push(out[out.len() - offset]);
                }
            }
        }
        if out.len() > total {
            return None;
        }
    }
    (out.len() == total).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(name: &str) -> Vec<u8> {
        let mut key = b"_http://127.0.0.1:8799\0\x01".to_vec();
        key.extend_from_slice(name.as_bytes());
        key
    }

    fn latin1(text: &str) -> Vec<u8> {
        let mut value = vec![1u8];
        value.extend_from_slice(text.as_bytes());
        value
    }

    fn put(out: &mut Vec<u8>, name: &str, value: &[u8]) {
        let key = key(name);
        out.push(1);
        out.push(key.len() as u8);
        out.extend_from_slice(&key);
        out.push(value.len() as u8);
        out.extend_from_slice(value);
    }

    fn batch(seq: u64, build: impl FnOnce(&mut Vec<u8>) -> u32) -> Vec<u8> {
        let mut body = Vec::new();
        let count = build(&mut body);
        let mut batch = seq.to_le_bytes().to_vec();
        batch.extend_from_slice(&count.to_le_bytes());
        batch.extend_from_slice(&body);
        batch
    }

    fn record(kind: u8, payload: &[u8]) -> Vec<u8> {
        let mut record = vec![0, 0, 0, 0];
        record.extend_from_slice(&(payload.len() as u16).to_le_bytes());
        record.push(kind);
        record.extend_from_slice(payload);
        record
    }

    fn collect(scan: impl FnOnce(&mut dyn FnMut(&[u8], u64, Option<&[u8]>))) -> Setting {
        let mut skin = Newest::default();
        let mut dress = Newest::default();
        scan(&mut |key, seq, value| match storage_key(key) {
            Some(name) if name == SKIN_KEY.as_bytes() => skin.offer(seq, value),
            Some(name) if name == DRESS_KEY.as_bytes() => dress.offer(seq, value),
            _ => {}
        });
        Setting { skin: skin.text(), dress: dress.text() }
    }

    #[test]
    fn the_last_write_in_the_log_wins_and_other_origins_are_ignored() {
        let first = batch(10, |body| {
            put(body, SKIN_KEY, &latin1("diwali"));
            put(body, "omb-drafts", &latin1("{}"));
            2
        });
        let second = batch(12, |body| {
            put(body, SKIN_KEY, &latin1("graphite"));
            1
        });
        let mut foreign = b"_https://example.org\0\x01omb-skin".to_vec();
        foreign.insert(0, 0);
        let mut log = record(1, &first);
        log.extend(record(1, &second));
        let setting = collect(|offer| scan_log(&log, offer));
        assert_eq!(setting.skin.as_deref(), Some("graphite"));
        assert_eq!(setting.festival(), None);
        assert!(storage_key(&foreign).is_none());
        assert!(storage_key(b"_https://example.org\0\x01omb-skin").is_none());
    }

    #[test]
    fn a_batch_split_over_fragments_is_put_back_together() {
        let whole = batch(1, |body| {
            put(body, SKIN_KEY, &latin1("halloween"));
            1
        });
        let (head, tail) = whole.split_at(20);
        let mut log = record(2, head);
        log.extend(record(4, tail));
        let setting = collect(|offer| scan_log(&log, offer));
        assert_eq!(setting.festival().as_deref(), Some("halloween"));
    }

    #[test]
    fn a_torn_record_at_the_end_is_ignored() {
        let whole = batch(1, |body| {
            put(body, SKIN_KEY, &latin1("diwali"));
            1
        });
        let mut log = record(1, &whole);
        let torn = record(1, &whole);
        log.extend_from_slice(&torn[..torn.len() - 5]);
        assert_eq!(collect(|offer| scan_log(&log, offer)).festival().as_deref(), Some("diwali"));
    }

    #[test]
    fn a_deletion_clears_the_value_and_dress_off_means_no_costume() {
        let put_then_delete = batch(5, |body| {
            put(body, SKIN_KEY, &latin1("halloween"));
            put(body, DRESS_KEY, &latin1("0"));
            let key = key(SKIN_KEY);
            body.push(0);
            body.push(key.len() as u8);
            body.extend_from_slice(&key);
            3
        });
        let log = record(1, &put_then_delete);
        let setting = collect(|offer| scan_log(&log, offer));
        assert_eq!(setting.skin, None);
        assert_eq!(setting.dress.as_deref(), Some("0"));

        let undressed = Setting { skin: Some("diwali".into()), dress: Some("0".into()) };
        assert_eq!(undressed.festival(), None);
        let dressed = Setting { skin: Some("diwali".into()), dress: None };
        assert_eq!(dressed.festival().as_deref(), Some("diwali"));
    }

    #[test]
    fn a_utf16_value_is_decoded() {
        let mut value = vec![0u8];
        for unit in "diwali".encode_utf16() {
            value.extend_from_slice(&unit.to_le_bytes());
        }
        let mut newest = Newest::default();
        newest.offer(1, Some(&value));
        assert_eq!(newest.text().as_deref(), Some("diwali"));
    }

    #[test]
    fn snappy_literals_and_copies_decompress() {
        // "abcabcabc": literal "abc" then a copy of length 6 at offset 3.
        let packed = [9u8, 2 << 2, b'a', b'b', b'c', ((6 - 4) << 2) | 1, 3];
        assert_eq!(snappy(&packed).as_deref(), Some(&b"abcabcabc"[..]));
        assert!(snappy(&[9, 2 << 2, b'a']).is_none());
        assert!(snappy(&[200, 200, 200, 200, 200, 8]).is_none());
    }

    fn data_block(rows: &[(Vec<u8>, u64, Option<&[u8]>)]) -> Vec<u8> {
        let mut block = Vec::new();
        for (user, seq, value) in rows {
            let mut internal = user.clone();
            internal.extend_from_slice(&((seq << 8) | u64::from(value.is_some())).to_le_bytes());
            let value = value.unwrap_or_default();
            block.push(0);
            block.push(internal.len() as u8);
            block.push(value.len() as u8);
            block.extend_from_slice(&internal);
            block.extend_from_slice(value);
        }
        block.extend_from_slice(&0u32.to_le_bytes());
        block.extend_from_slice(&1u32.to_le_bytes());
        block
    }

    #[test]
    fn a_table_is_read_whether_or_not_its_block_is_compressed() {
        let skin = latin1("diwali");
        let rows = [(key(SKIN_KEY), 7, Some(skin.as_slice()))];
        let block = data_block(&rows);

        let mut table = block.clone();
        table.extend_from_slice(&[0, 0, 0, 0, 0]);
        let index = {
            let mut handle = vec![0u8, block.len() as u8];
            let mut index = vec![0u8, 1, handle.len() as u8, b'k'];
            index.append(&mut handle);
            index.extend_from_slice(&0u32.to_le_bytes());
            index.extend_from_slice(&1u32.to_le_bytes());
            index
        };
        let index_at = table.len();
        table.extend_from_slice(&index);
        table.extend_from_slice(&[0, 0, 0, 0, 0]);
        let mut footer = vec![0u8, 0, index_at as u8, index.len() as u8];
        footer.resize(40, 0);
        footer.extend_from_slice(&TABLE_MAGIC);
        table.extend_from_slice(&footer);

        let setting = collect(|offer| scan_table(&table, offer));
        assert_eq!(setting.festival().as_deref(), Some("diwali"));

        let mut packed = vec![block.len() as u8];
        let mut rest = block.as_slice();
        while !rest.is_empty() {
            let take = rest.len().min(60);
            packed.push(((take - 1) as u8) << 2);
            packed.extend_from_slice(&rest[..take]);
            rest = &rest[take..];
        }
        assert_eq!(snappy(&packed).as_deref(), Some(block.as_slice()));
    }

    #[test]
    fn a_missing_directory_reads_as_nothing() {
        assert!(read(Path::new("/nonexistent/omb-leveldb")).is_none());
        assert!(stamps(Path::new("/nonexistent/omb-leveldb")).is_empty());
    }
}
