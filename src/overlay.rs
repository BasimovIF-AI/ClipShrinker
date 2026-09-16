use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

pub const OVERLAY_MAGIC: &[u8; 16] = b"VCONV_OVERLAY_V1";
pub const FOOTER_SIZE: usize = 48;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverlayFooter {
    pub magic: [u8; 16],
    pub ffmpeg_major: u32,
    pub ffmpeg_minor: u32,
    pub ffmpeg_patch: u32,
    pub app_build: u32,
    pub raw_ffmpeg_size: u64,
    pub payload_size: u64,
}

impl OverlayFooter {
    pub fn new(
        ffmpeg_major: u32,
        ffmpeg_minor: u32,
        ffmpeg_patch: u32,
        app_build: u32,
        raw_ffmpeg_size: u64,
        payload_size: u64,
    ) -> Self {
        Self {
            magic: *OVERLAY_MAGIC,
            ffmpeg_major,
            ffmpeg_minor,
            ffmpeg_patch,
            app_build,
            raw_ffmpeg_size,
            payload_size,
        }
    }

    pub fn to_bytes(&self) -> [u8; FOOTER_SIZE] {
        let mut buf = [0u8; FOOTER_SIZE];
        buf[0..16].copy_from_slice(&self.magic);
        buf[16..20].copy_from_slice(&self.ffmpeg_major.to_le_bytes());
        buf[20..24].copy_from_slice(&self.ffmpeg_minor.to_le_bytes());
        buf[24..28].copy_from_slice(&self.ffmpeg_patch.to_le_bytes());
        buf[28..32].copy_from_slice(&self.app_build.to_le_bytes());
        buf[32..40].copy_from_slice(&self.raw_ffmpeg_size.to_le_bytes());
        buf[40..48].copy_from_slice(&self.payload_size.to_le_bytes());
        buf
    }

    pub fn from_bytes(buf: &[u8; FOOTER_SIZE]) -> Option<Self> {
        let mut magic = [0u8; 16];
        magic.copy_from_slice(&buf[0..16]);
        if &magic != OVERLAY_MAGIC {
            return None;
        }
        let ffmpeg_major = u32::from_le_bytes(buf[16..20].try_into().unwrap());
        let ffmpeg_minor = u32::from_le_bytes(buf[20..24].try_into().unwrap());
        let ffmpeg_patch = u32::from_le_bytes(buf[24..28].try_into().unwrap());
        let app_build = u32::from_le_bytes(buf[28..32].try_into().unwrap());
        let raw_ffmpeg_size = u64::from_le_bytes(buf[32..40].try_into().unwrap());
        let payload_size = u64::from_le_bytes(buf[40..48].try_into().unwrap());

        Some(Self {
            magic,
            ffmpeg_major,
            ffmpeg_minor,
            ffmpeg_patch,
            app_build,
            raw_ffmpeg_size,
            payload_size,
        })
    }
}

#[derive(Debug, Clone)]
pub struct OverlayInfo {
    pub footer: OverlayFooter,
    pub pe_size: u64,
    pub payload_offset: u64,
    pub payload_size: u64,
}

/// Reads and verifies the overlay footer at the end of the specified executable.
pub fn read_overlay(exe_path: &Path) -> io::Result<Option<OverlayInfo>> {
    let mut file = File::open(exe_path)?;
    let total_len = file.metadata()?.len();
    if total_len < FOOTER_SIZE as u64 {
        return Ok(None);
    }

    file.seek(SeekFrom::End(-(FOOTER_SIZE as i64)))?;
    let mut footer_buf = [0u8; FOOTER_SIZE];
    file.read_exact(&mut footer_buf)?;

    if let Some(footer) = OverlayFooter::from_bytes(&footer_buf) {
        if total_len >= FOOTER_SIZE as u64 + footer.payload_size {
            let payload_offset = total_len - FOOTER_SIZE as u64 - footer.payload_size;
            return Ok(Some(OverlayInfo {
                pe_size: payload_offset,
                payload_offset,
                payload_size: footer.payload_size,
                footer,
            }));
        }
    }

    Ok(None)
}

/// Parses the Windows PE headers to find the physical end of all PE sections.
/// This determines the exact PE boundary even when no overlay has been appended yet.
pub fn get_pe_boundary(exe_path: &Path) -> io::Result<u64> {
    let mut file = File::open(exe_path)?;
    let mut header = vec![0u8; 4096];
    let bytes_read = file.read(&mut header)?;
    if bytes_read < 0x40 || &header[0..2] != b"MZ" {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Not a valid PE file (missing MZ)"));
    }

    let pe_offset = u32::from_le_bytes([header[0x3C], header[0x3D], header[0x3E], header[0x3F]]) as usize;
    if bytes_read < pe_offset + 24 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid PE offset in DOS header"));
    }
    if &header[pe_offset..pe_offset + 4] != b"PE\0\0" {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid PE signature"));
    }

    let num_sections = u16::from_le_bytes([header[pe_offset + 6], header[pe_offset + 7]]) as usize;
    let opt_header_size = u16::from_le_bytes([header[pe_offset + 20], header[pe_offset + 21]]) as usize;
    let section_headers_offset = pe_offset + 24 + opt_header_size;

    // Read more if section headers extend beyond initial 4096 bytes
    let needed = section_headers_offset + num_sections * 40;
    if bytes_read < needed {
        header.resize(needed, 0);
        file.seek(SeekFrom::Start(bytes_read as u64))?;
        file.read_exact(&mut header[bytes_read..needed])?;
    }

    let mut max_end = 0usize;
    for i in 0..num_sections {
        let sec_offset = section_headers_offset + i * 40;
        let size_of_raw_data = u32::from_le_bytes([
            header[sec_offset + 16],
            header[sec_offset + 17],
            header[sec_offset + 18],
            header[sec_offset + 19],
        ]) as usize;
        let pointer_to_raw_data = u32::from_le_bytes([
            header[sec_offset + 20],
            header[sec_offset + 21],
            header[sec_offset + 22],
            header[sec_offset + 23],
        ]) as usize;
        let end = pointer_to_raw_data + size_of_raw_data;
        if end > max_end {
            max_end = end;
        }
    }

    if max_end == 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "No PE sections found"));
    }

    Ok(max_end as u64)
}

/// Builds a new executable by combining the PE stub from `base_exe_path`,
/// the compressed payload from `payload_path`, and the metadata footer,
/// writing the result to `output_path`.
pub fn build_overlay_exe(
    base_exe_path: &Path,
    payload_path: &Path,
    footer: &OverlayFooter,
    output_path: &Path,
) -> io::Result<()> {
    // 1. Determine base PE stub length
    let pe_size = if let Some(existing_overlay) = read_overlay(base_exe_path)? {
        existing_overlay.pe_size
    } else {
        get_pe_boundary(base_exe_path)?
    };

    let mut base_file = File::open(base_exe_path)?;
    let mut payload_file = File::open(payload_path)?;
    let payload_len = payload_file.metadata()?.len();

    let mut verified_footer = *footer;
    verified_footer.payload_size = payload_len;

    // Use a temp file in the same directory for atomic creation
    let temp_output = output_path.with_extension("tmp_pack");
    if temp_output.exists() {
        let _ = fs::remove_file(&temp_output);
    }

    {
        let mut out = File::create(&temp_output)?;

        // Stream PE stub (exact pe_size bytes)
        let mut reader = (&mut base_file).take(pe_size);
        io::copy(&mut reader, &mut out)?;

        // Stream payload
        io::copy(&mut payload_file, &mut out)?;

        // Write 48-byte footer
        out.write_all(&verified_footer.to_bytes())?;
        out.flush()?;
    }

    // Atomic move to output_path using rename trick to prevent locking issues
    if output_path.exists() {
        let old = output_path.with_extension("exe.old_pack");
        let _ = fs::remove_file(&old);
        let _ = fs::rename(output_path, &old);
        let _ = fs::remove_file(&old);
    }
    fs::rename(&temp_output, output_path)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_overlay_footer_size() {
        assert_eq!(std::mem::size_of::<OverlayFooter>(), FOOTER_SIZE);
    }

    #[test]
    fn test_overlay_footer_roundtrip() {
        let footer = OverlayFooter::new(9, 1, 0, 5, 102856192, 27494452);
        let bytes = footer.to_bytes();
        assert_eq!(bytes.len(), FOOTER_SIZE);

        let parsed = OverlayFooter::from_bytes(&bytes).expect("Failed to parse valid footer");
        assert_eq!(parsed.magic, *OVERLAY_MAGIC);
        assert_eq!(parsed.ffmpeg_major, 9);
        assert_eq!(parsed.ffmpeg_minor, 1);
        assert_eq!(parsed.ffmpeg_patch, 0);
        assert_eq!(parsed.app_build, 5);
        assert_eq!(parsed.raw_ffmpeg_size, 102856192);
        assert_eq!(parsed.payload_size, 27494452);
    }

    #[test]
    fn test_overlay_footer_invalid_magic() {
        let mut bytes = [0u8; FOOTER_SIZE];
        bytes[0..16].copy_from_slice(b"INVALID_MAGIC!!!");
        assert!(OverlayFooter::from_bytes(&bytes).is_none());
    }
}
