// For handling cli Args
use clap::{Command, Arg, ArgAction, ArgMatches};

// Database handling
use rusqlite::{Connection, functions::FunctionFlags, Error};
use std::sync::Arc;
type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

// For searching through database and filter out folders to avoid
use regex::Regex;

// For indexing files
use mountpoints;
use walkdir::WalkDir;

// For loading animations
use spinners::{Spinner, Spinners};

// For type annotations
use std::path::{Path, PathBuf};


fn locate_regex(expr: &str, db_conn: &mut Connection) -> Vec<String> {
    let mut stmt: rusqlite::Statement<'_> = db_conn.prepare("SELECT PATH FROM FILES WHERE PATH REGEXP ?1").unwrap();
    let matches = stmt.query_map([expr], |row| row.get::<usize, String>(0)).unwrap();
    let vec_matches: &Vec<String> = &matches.map(|path| path.unwrap()).collect();
    let _ = stmt.finalize();
    return vec_matches.to_owned()
}


fn update(db_conn: &mut Connection) -> () {
    let avoid_rgx: regex::Regex = Regex::new(r"^/(boot|dev|proc|sys|tmp)").unwrap();
    let filesystems = ["FAT12", "FAT16", "FAT32", "exFAT", "NTFS", "fuseblk","ReFS", "HFS", "HFS+", "HPFS", "APFS", "UFS", "ext2", "ext3", "ext4"];

    let mounts: Vec<mountpoints::MountInfo> = mountpoints::mountinfos()
        .unwrap()
        .into_iter()
        .filter(|mount| !mount.dummy && filesystems.contains(&mount.format.clone().unwrap().as_str()))
        .collect();

    let mut files = vec!();

    for mount in &mounts {
        let mountpath: &Path = &mount.path;
        println!("Indexing {}", mountpath.display());
        let paths: Vec<walkdir::DirEntry> = WalkDir::new(mountpath)
            .same_file_system(true)
            .into_iter()
            .filter_map(|e| e.ok())
            .collect();

        let mut entry_as_path: PathBuf;
        for entry in paths {
            entry_as_path = entry.into_path();
            if !avoid_rgx.is_match(&format!("{}", entry_as_path.display())){
                files.push(entry_as_path.clone());
           }
        }
    }
    // println!("{:?}", files);
    println!("Adding indexed files to database");
    let _ = db_conn.execute("DROP TABLE IF EXISTS FILES", []);
    let _ = db_conn.execute("CREATE TABLE FILES(PATH TEXT)", []);
    let tx = db_conn.transaction().unwrap();
    let mut stmt = tx.prepare("INSERT INTO FILES (PATH) VALUES(?1)").unwrap();
    for entry in &files {
        let _ = stmt.execute([entry.to_str().unwrap()]);
    }
    let _ = stmt.finalize();
    let _ = tx.commit();
    ()
}


fn main() {

    let cli_args: Command = Command::new("rlocate")
        .arg(Arg::new("update")
             .long("update")
             .short('u')
             .action(ArgAction::SetTrue)
             .help("Update the database")
             .required(false))
        .arg(Arg::new("locate")
             .long("locate")
             .short('l')
             .action(ArgAction::Set)
             .help("Locate file from database")
             .allow_hyphen_values(true))
        .arg(Arg::new("count")
             .long("count")
             .short('c')
             .action(ArgAction::SetTrue)
             .help("Whether to count the files matching or display them")
             );

    let mut db_conn: Connection = Connection::open("./test.db").unwrap();
    let _ = db_conn.create_scalar_function(
        "regexp",
        2,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        move |ctx| {
            assert_eq!(ctx.len(), 2, "called with unexpected number of arguments");
            let regexp: Arc<Regex> = ctx.get_or_create_aux(0, |vr| -> Result<_, BoxError> {
                Ok(Regex::new(vr.as_str()?)?)
            })?;
            let is_match = {
                let text = ctx
                    .get_raw(1)
                    .as_str()
                    .map_err(|e| Error::UserFunctionError(e.into()))?;

                regexp.is_match(text)
            };

            Ok(is_match)
        },
    );

    let cli_matches = cli_args.get_matches();
    if cli_matches.get_flag("update") {
        update(&mut db_conn)
    }


}
