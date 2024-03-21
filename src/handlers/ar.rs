/* SPDX-License-Identifier: GPL-3.0-or-later */

use anyhow::{Result, anyhow};
use log::debug;
use std::fs::File;
use std::io::{BufWriter, Read, Write, Seek};
use std::path::{Path, PathBuf};

use crate::handlers::FileProcess;

const MAGIC: &[u8] = b"!<arch>\n";

const FILE_HEADER_LENGTH: usize = 60;
const FILE_HEADER_MAGIC: &[u8] = &[0o140, 0o012];

pub fn filter(path: &Path) -> Result<bool> {
    Ok(path.extension().is_some_and(|x| x == "a"))
}

pub fn handler(fp: &mut FileProcess) -> Result<(PathBuf, File, bool)> {
    let mut have_mod = false;

    let mut buf = [0; MAGIC.len()];
    fp.input.read_exact(&mut buf)?;
    if buf != MAGIC {
        return Err(anyhow!("{}: wrong magic ({:?})", fp.input_path.display(), buf));
    }

    let (output_path, output) = fp.open_output()?;
    let mut output = BufWriter::new(output);

    output.write_all(&buf)?;

    loop {
        let pos = fp.input.stream_position()?;
        let mut buf = [0; FILE_HEADER_LENGTH];

        debug!("{}: reading file header at offset {}", fp.input_path.display(), pos);

        match fp.input.read(&mut buf)? {
            0 => break,
            FILE_HEADER_LENGTH => {},
            n => {
                return Err(anyhow!("{}: short read of {} bytes at offset {}",
                                   fp.input_path.display(), n, pos));
            }
        }

        // https://en.wikipedia.org/wiki/Ar_(Unix)
        // from   to     Name                      Format
        // 0      15     File name                 ASCII
        // 16     27     File modification date    Decimal
        // 28     33     Owner ID                  Decimal
        // 34     39     Group ID                  Decimal
        // 40     47     File mode                 Octal
        // 48     57     File size in bytes        Decimal
        // 58     59     File magic                \140\012

        if &buf[58..] != FILE_HEADER_MAGIC {
            return Err(anyhow!("{}: wrong magic in file header at offset {}: {:?} != {:?}",
                               fp.input_path.display(), pos, &buf[58..], FILE_HEADER_MAGIC));
        }

        let name = std::str::from_utf8(&buf[0..16])?.trim_end_matches(' ');

        let size = std::str::from_utf8(&buf[48..58])?.trim_end_matches(' ');
        let size = size.parse::<u32>()?;

        if name == "//" {
            // System V/GNU table of long filenames
            debug!("{}: long filename index, size={}", fp.input_path.display(), size);
        } else {
            let mtime = std::str::from_utf8(&buf[16..28])?.trim_end_matches(' ');
            let mtime = mtime.parse::<u64>()?;

            let uid = std::str::from_utf8(&buf[28..34])?.trim_end_matches(' ');
            let uid = uid.parse::<u64>()?;

            let gid = std::str::from_utf8(&buf[34..40])?.trim_end_matches(' ');
            let gid = gid.parse::<u64>()?;

            let mode = std::str::from_utf8(&buf[40..48])?.trim_end_matches(' ');
            let mode = mode.parse::<u64>()?;

            debug!("{}: file {:?}, mtime={}, {}:{}, mode={:o}, size={}",
                   fp.input_path.display(), name, mtime, uid, gid, mode, size);

            if fp.options.source_date_epoch.is_some() && mtime > fp.options.source_date_epoch.unwrap() {
                let source_date_epoch_str = format!("{:<12}", fp.options.source_date_epoch.unwrap());

                buf[16..28].copy_from_slice(source_date_epoch_str.as_bytes());
                have_mod = true;
            }

            if uid != 0 || gid != 0 {
                buf[28..34].copy_from_slice(b"0     ");
                buf[34..40].copy_from_slice(b"0     ");
                have_mod = true;
            }
        }

        output.write_all(&buf)?;

        let padded_size = size + size % 2;

        let mut buf = vec![0; padded_size.try_into().unwrap()];
        fp.input.read_exact(&mut buf)?;

        output.write_all(&buf)?;
    }

    output.flush()?;

    Ok((output_path, output.into_inner()?, have_mod))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_a() {
        assert!( filter(Path::new("/some/path/libfoobar.a")).unwrap());
        assert!(!filter(Path::new("/some/path/libfoobar.aa")).unwrap());
        assert!( filter(Path::new("/some/path/libfoobar.a.a")).unwrap());
        assert!(!filter(Path::new("/some/path/libfoobara")).unwrap());
        assert!(!filter(Path::new("/some/path/a")).unwrap());
        assert!(!filter(Path::new("/some/path/a_a")).unwrap());
        assert!(!filter(Path::new("/")).unwrap());
    }
}
