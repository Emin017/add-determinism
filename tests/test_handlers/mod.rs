mod test_ar;
mod test_javadoc;
mod test_pyc;

use anyhow::Result;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use tempfile::TempDir;

use add_determinism::options;
use add_determinism::handlers;

fn prepare_dir(path: &str) -> Result<(Box<TempDir>, Box<PathBuf>)> {
    let dir = TempDir::new()?;
    let input_path = dir.path().join(Path::new(path).file_name().unwrap());
    fs::copy(path, &input_path)?;
    Ok((Box::new(dir), Box::new(input_path)))
}

fn make_handler(
    source_date_epoch: i64,
    func: handlers::HandlerBoxed,
) -> Result<Box<dyn handlers::Processor>> {

    let cfg = Rc::new(options::Config::empty(source_date_epoch));
    let mut handler = func(&cfg);
    handler.initialize()?;
    Ok(handler)
}

struct Trivial {}

impl Trivial {
    pub fn boxed() -> Box<dyn handlers::Processor> {
        Box::new(Self {})
    }
}
impl handlers::Processor for Trivial {
    fn name(&self) -> &str {
        "trivial"
    }

    fn filter(&self, _path: &Path) -> Result<bool> {
        Ok(true)
    }

    fn process(&self, _input_path: &Path) -> Result<bool> {
        Ok(true)
    }
}

#[test]
fn test_input_output_helper_drop() {
    let (_dir, input) = prepare_dir("tests/cases/libempty.a").unwrap();

    let (mut helper, _) = handlers::InputOutputHelper::open(&*input).unwrap();
    helper.open_output().unwrap();

    let output_path = helper.output_path.as_ref().unwrap().clone();

    assert!(output_path.exists());
    drop(helper);
    assert!(!output_path.exists());
}

#[test]
fn test_inode_map() {
    let (dir, _input) = prepare_dir("tests/cases/libempty.a").unwrap();

    let mut handlers = vec![ Trivial::boxed() ];
    let mut cache = handlers::inodes_seen();

    let mods = handlers::process_file_or_dir(&handlers, &mut cache, dir.path(), None).unwrap();
    assert_eq!(mods, 1);

    let mods = handlers::process_file_or_dir(&handlers, &mut cache, dir.path(), None).unwrap();
    assert_eq!(mods, 0);

    assert_eq!(cache.len(), 1);

    handlers.push(Trivial::boxed());

    let mods = handlers::process_file_or_dir(&handlers, &mut cache, dir.path(), None).unwrap();
    assert_eq!(mods, 1);

    let mods = handlers::process_file_or_dir(&handlers, &mut cache, dir.path(), None).unwrap();
    assert_eq!(mods, 0);

    assert_eq!(cache.len(), 1);
}

#[test]
fn test_inode_map_2() {
    let (dir, _input) = prepare_dir("tests/cases/testrelro.a").unwrap();

    let cfg = Rc::new(options::Config::empty(111));
    let ar = handlers::ar::Ar::boxed(&cfg);

    let handlers = vec![ar];
    let mut cache = handlers::inodes_seen();

    let mods = handlers::process_file_or_dir(&handlers, &mut cache, dir.path(), None).unwrap();
    assert_eq!(mods, 1);

    let mods = handlers::process_file_or_dir(&handlers, &mut cache, dir.path(), None).unwrap();
    assert_eq!(mods, 0); // The file was already processed, so no change

    // The inode changes because we rewrite the file
    assert_eq!(cache.len(), 2);
}

fn test_corpus_file(handler: Box<dyn handlers::Processor>, filename: &str) {
    let filename = Path::new(filename);
    let (_dir, input) = prepare_dir(filename.to_str().unwrap()).unwrap();

    let ext = filename.extension().unwrap().to_str().unwrap().to_owned();
    let fixed = filename.with_extension(ext + ".fixed");
    let have_mod = fixed.exists();

    assert!(handler.filter(&*input).unwrap());
    assert_eq!(handler.process(&*input).unwrap(), have_mod);

    let mut data_expected = vec![];
    fs::File::open(
        if have_mod { &fixed } else { filename }
    ).unwrap()
        .read_to_end(&mut data_expected).unwrap();

    let mut data_output: Vec<u8> = vec![];
    fs::File::open(&*input).unwrap()
        .read_to_end(&mut data_output).unwrap();

    assert_eq!(data_output, data_expected);
}
