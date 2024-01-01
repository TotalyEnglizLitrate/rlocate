// For handling cli Args
use clap::{arg, command, Command, ArgMatches, ArgAction};

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

// For figuring out/creating database path
use dirs;
use std::fs::create_dir_all;


fn locate_regex(expr: &str, db_conn: &mut Connection) -> Vec<String> {
    let mut stmt: rusqlite::Statement<'_> = db_conn.prepare("SELECT PATH FROM FILES WHERE PATH REGEXP ?1").unwrap();
    let mut spinner = Spinner::new(Spinners::Arc, "Searching for files...".to_string());
    let matches = stmt.query_map([expr], |row| row.get::<usize, String>(0)).unwrap();
    let vec_matches: &Vec<String> = &matches.map(|path| path.unwrap()).collect();
    let _ = stmt.finalize();
    spinner.stop_and_persist("✓", format!("Found {} files", vec_matches.len()));
    return vec_matches.to_owned()
}


fn update(db_conn: &mut Connection) -> () {
    let mut spinner = Spinner::new(Spinners::Arc, "Looking for mountpoints".to_string());
    let avoid_rgx: regex::Regex = Regex::new(r"^/(boot|dev|proc|sys|tmp)").unwrap();
    let filesystems = ["FAT12", "FAT16", "FAT32", "exFAT", "NTFS", "fuseblk","ReFS", "HFS", "HFS+", "HPFS", "APFS", "UFS", "ext2", "ext3", "ext4"];

    let _tmp_mounts: Vec<mountpoints::MountInfo> = mountpoints::mountinfos()
        .unwrap()
        .into_iter()
        .filter(|mount| !mount.dummy && filesystems.contains(&mount.format.clone().unwrap().as_str()))
        .collect();
 
    let mut mounts: Vec<mountpoints::MountInfo> = vec!();
    let mut count: u8;
    for mount in &_tmp_mounts {
        count = 0;
        for mount1 in &_tmp_mounts {
            if mount.path == mount1.path {
                count += 1;
                continue;
            }
            if mount.path.starts_with(mount1.path.clone()) {
                break;
            }
            count += 1;
        }
        if count == _tmp_mounts.len().try_into().unwrap() {
            mounts.push(mount.clone())
        }
    }
    spinner.stop_and_persist("✓", "Found mounts".to_string());

    let mut files = vec!();
    for mount in &mounts {
        let mountpath: &Path = &mount.path;
        let mut spinner = Spinner::new(Spinners::Arc, format!("Indexing {}", mountpath.display()));
        let paths: Vec<walkdir::DirEntry> = WalkDir::new(mountpath)
            .same_file_system(false)
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
        spinner.stop_and_persist("✓", format!("Finished indexing {}", mountpath.display()));
    }
    let mut spinner = Spinner::new(Spinners::Arc, "Adding indexed files to database".to_string());
    let _ = db_conn.execute("DROP TABLE IF EXISTS FILES", []);
    let _ = db_conn.execute("CREATE TABLE FILES(PATH TEXT)", []);
    let tx = db_conn.transaction().unwrap();
    let mut stmt = tx.prepare("INSERT INTO FILES (PATH) VALUES(?1)").unwrap();
    for entry in &files {
        let _ = stmt.execute([entry.to_str().unwrap()]);
    }
    let _ = stmt.finalize();
    let _ = tx.commit();
    spinner.stop_and_persist("✓", "Finished adding files to database".to_string());
    ()
}


fn main() {
    let escape = Regex::new(r"(\\|\[|\]|[.^$|(){}])").unwrap();

    let cli_matches: ArgMatches = command!()
        .arg(
            arg!(-u --update "Flag to set whtther to update database")
            .action(ArgAction::SetTrue)
            .required(false)
            )
        .subcommand(
            Command::new("find")
            .arg(
                arg!(<String> "Plain string or regex pattern to search[pass -r(--regexp) to search with regex]")
                )
            .arg(
                arg!(-c --count "Flag to set whether to return file paths or just the count of matched files")
                .action(ArgAction::SetTrue)
                .required(false)
                )
            .arg(
                arg!(-r --regexp "Flag to set whether search pattern is regex")
                .action(ArgAction::SetTrue)
                .required(false)
                )
            )
        .get_matches();

    let mut db_path: PathBuf;
    db_path = dirs::data_dir().unwrap().to_owned();
    db_path.push("rlocate");
    if !db_path.exists() {
       create_dir_all(db_path.to_str().unwrap()).unwrap();
    }
    db_path.push("files.db");
    let mut db_conn: Connection = Connection::open(format!("{}", db_path.display())).unwrap();
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

    if cli_matches.get_flag("update") {
        update(&mut db_conn)
    }

    if let Some(locate_matches) = cli_matches.subcommand_matches("find") {
        let path_matches: Vec<String>;
        let expr: String;
        if locate_matches.get_flag("regexp") {
            expr = locate_matches.to_owned().remove_one("String").unwrap();
            path_matches = locate_regex(&expr, &mut db_conn);
        }
        else {
            expr = format!(r"^.*{}.*$", escape.replace_all(&locate_matches.to_owned().remove_one::<String>("String").unwrap(), r"\$1"));
            path_matches = locate_regex(&expr, &mut db_conn);
        }
        if locate_matches.get_flag("count") {
            println!("{} matches found for {}", path_matches.len(), expr)
        }
        else {
            for path in path_matches {
                println!("{}", path);
            }
        }
    }
}
