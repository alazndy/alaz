use crate::cave_map::KnownTiles;
use alaz_sim::{AgentId, World, cave_terrain::CaveTile, caves::Cell};
use std::fs;
use std::io::{self, Read};
use std::path::Path;

pub(super) const FILE_NAME: &str = "cave-map.alz";
const MAGIC: &[u8; 4] = b"ACM1";
const TILE_LEN: usize = 4 + 4 + 2 + 1;
type Fingerprint = [u8; 16];

struct Checkpoint<'a> {
    fingerprint: Fingerprint,
    player: AgentId,
    tiles: &'a [u8],
}

// This header already includes the world format and a checksum of its full payload.
pub(super) fn save_fingerprint(bytes: &[u8]) -> Option<Fingerprint> {
    bytes
        .first_chunk()
        .copied()
        .filter(|header: &Fingerprint| &header[..4] == b"ALAZ")
}

pub(super) fn read_fingerprint(path: &Path) -> Option<Fingerprint> {
    let mut header = Fingerprint::default();
    fs::File::open(path).ok()?.read_exact(&mut header).ok()?;
    save_fingerprint(&header)
}

pub(super) fn update(
    dir: &Path,
    fingerprint: Fingerprint,
    backup: Option<Fingerprint>,
    player: AgentId,
    seen: &KnownTiles,
) -> io::Result<()> {
    let path = dir.join(FILE_NAME);
    let old = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(error),
    };
    if old.is_empty() && seen.is_empty() {
        return Ok(());
    }

    let mut bytes = MAGIC.to_vec();
    for record in decode(&old).unwrap_or_default() {
        let previous = Some(record.fingerprint) == backup;
        let current = record.fingerprint == fingerprint;
        if (previous || current) && !(current && record.player == player) {
            write_header(
                &mut bytes,
                record.fingerprint,
                record.player,
                record.tiles.len() / TILE_LEN,
            )?;
            bytes.extend_from_slice(record.tiles);
        }
    }
    if !seen.is_empty() {
        write_header(&mut bytes, fingerprint, player, seen.len())?;
        for (tile, cell) in seen {
            bytes.extend_from_slice(&tile.x.to_le_bytes());
            bytes.extend_from_slice(&tile.y.to_le_bytes());
            bytes.extend_from_slice(&tile.z.to_le_bytes());
            bytes.push(*cell as u8);
        }
    }
    bytes.extend_from_slice(&checksum(&bytes[MAGIC.len()..]).to_le_bytes());
    if bytes == old {
        return Ok(());
    }
    super::write_atomic(&path, &bytes)
}

pub(super) fn load(dir: &Path, bytes: &[u8], world: &World, player: AgentId) -> KnownTiles {
    let load_matching = || {
        let fingerprint = save_fingerprint(bytes)?;
        let file = fs::read(dir.join(FILE_NAME)).ok()?;
        let record = decode(&file)?
            .into_iter()
            .find(|record| record.fingerprint == fingerprint && record.player == player)?;
        let mut seen = KnownTiles::new();
        for tile in record.tiles.as_chunks::<TILE_LEN>().0 {
            let x = i32::from_le_bytes(tile[..4].try_into().ok()?);
            let y = i32::from_le_bytes(tile[4..8].try_into().ok()?);
            let z = i16::from_le_bytes(tile[8..10].try_into().ok()?);
            if !world.map.in_bounds(x, y) || tile[10] > Cell::Rubble as u8 {
                return None;
            }
            if seen
                .insert(CaveTile { x, y, z }, Cell::from_u8(tile[10]))
                .is_some()
            {
                return None;
            }
        }
        Some(seen)
    };
    load_matching().unwrap_or_default()
}

fn write_header(
    bytes: &mut Vec<u8>,
    fingerprint: Fingerprint,
    player: AgentId,
    count: usize,
) -> io::Result<()> {
    let count = u32::try_from(count)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "cave map too large"))?;
    bytes.extend_from_slice(&fingerprint);
    bytes.extend_from_slice(&player.0.to_le_bytes());
    bytes.extend_from_slice(&count.to_le_bytes());
    Ok(())
}

fn decode(bytes: &[u8]) -> Option<Vec<Checkpoint<'_>>> {
    let (mut body, sum) = bytes.strip_prefix(MAGIC)?.split_last_chunk::<8>()?;
    if checksum(body) != u64::from_le_bytes(*sum) {
        return None;
    }
    let mut records = Vec::new();
    while !body.is_empty() {
        let (header, rest) = body.split_first_chunk::<24>()?;
        let fingerprint = save_fingerprint(header)?;
        let player = AgentId(u32::from_le_bytes(header[16..20].try_into().ok()?));
        let count = u32::from_le_bytes(header[20..24].try_into().ok()?) as usize;
        let (tiles, remaining) = rest.split_at_checked(count.checked_mul(TILE_LEN)?)?;
        records.push(Checkpoint {
            fingerprint,
            player,
            tiles,
        });
        body = remaining;
    }
    Some(records)
}

fn checksum(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}
