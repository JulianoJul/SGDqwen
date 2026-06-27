use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use sha2::{Sha256, Digest};
use uuid::Uuid;

pub struct Storage {
    pub base_path: PathBuf,
    pub files_path: PathBuf,
}

impl Storage {
    pub fn new(base_path: &Path) -> Self {
        let files_path = base_path.join("files");
        Self { base_path: base_path.to_path_buf(), files_path }
    }

    pub fn init(&self) -> io::Result<()> {
        fs::create_dir_all(&self.base_path)?;
        fs::create_dir_all(&self.files_path)?;
        Ok(())
    }

    pub fn ensure_subdir(&self, subdir: &str) -> io::Result<()> {
        fs::create_dir_all(self.files_path.join(subdir))
    }

    pub fn import_file(&self, source: &Path, subdir: &str) -> io::Result<(String, String, i64)> {
        let original_name = source.file_name().unwrap_or_default().to_string_lossy().to_string();
        let ext = source.extension().unwrap_or_default().to_string_lossy().to_string().to_lowercase();
        let uuid = Uuid::new_v4();
        let filename = if ext.is_empty() { uuid.to_string() } else { format!("{}.{}", uuid, ext) };
        let subdir_path = self.files_path.join(subdir);
        fs::create_dir_all(&subdir_path)?;
        let dest = subdir_path.join(&filename);
        fs::copy(source, &dest)?;
        let size = fs::metadata(&dest)?.len() as i64;
        let rel_path = format!("files/{}/{}", subdir, filename);
        Ok((rel_path, original_name, size))
    }

    pub fn delete_file(&self, relative_path: &str) -> io::Result<()> {
        let full_path = self.get_full_path(relative_path);
        if full_path.exists() { fs::remove_file(full_path)?; }
        Ok(())
    }

    pub fn get_full_path(&self, relative_path: &str) -> PathBuf {
        self.base_path.join(relative_path)
    }

    #[allow(dead_code)]
    pub fn copy_to(&self, relative_path: &str, dest: &Path) -> io::Result<u64> {
        let src = self.get_full_path(relative_path);
        fs::copy(&src, dest)
    }

    pub fn calculate_checksum(&self, path: &Path) -> io::Result<String> {
        let mut file = fs::File::open(path)?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 65536];
        loop {
            let bytes_read = file.read(&mut buffer)?;
            if bytes_read == 0 { break; }
            hasher.update(&buffer[..bytes_read]);
        }
        Ok(format!("{:x}", hasher.finalize()))
    }

    pub fn backup_all(&self, dest_dir: &Path) -> io::Result<()> {
        fs::create_dir_all(dest_dir)?;
        let db_path = self.base_path.join("documents.db");
        if db_path.exists() {
            let backup_db = dest_dir.join("documents.db");
            fs::copy(&db_path, &backup_db)?;
        }
        let settings_path = self.base_path.join("settings.json");
        if settings_path.exists() {
            fs::copy(&settings_path, dest_dir.join("settings.json"))?;
        }
        let files_dest = dest_dir.join("files");
        fn copy_dir(src: &Path, dst: &Path) -> io::Result<()> {
            fs::create_dir_all(dst)?;
            for entry in fs::read_dir(src)? {
                let entry = entry?;
                let ty = entry.file_type()?;
                if ty.is_dir() {
                    copy_dir(&entry.path(), &dst.join(entry.file_name()))?;
                } else {
                    fs::copy(entry.path(), dst.join(entry.file_name()))?;
                }
            }
            Ok(())
        }
        if self.files_path.exists() {
            copy_dir(&self.files_path, &files_dest)?;
        }
        Ok(())
    }
}
