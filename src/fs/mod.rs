use xx_core::os::dirent;
use xx_core::os::stat::*;

use super::*;

pub mod readdir;

pub use readdir::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FileType(dirent::FileType);

impl FileType {
	#[must_use]
	pub fn is_dir(&self) -> bool {
		self.0 == dirent::FileType::Directory
	}

	#[must_use]
	pub fn is_file(&self) -> bool {
		self.0 == dirent::FileType::Regular
	}

	#[must_use]
	pub fn is_symlink(&self) -> bool {
		self.0 == dirent::FileType::Link
	}
}

#[allow(missing_copy_implementations)]
#[derive(Clone)]
pub struct Metadata(Statx);

#[allow(clippy::len_without_is_empty, clippy::missing_panics_doc)]
impl Metadata {
	#[allow(clippy::unwrap_used)]
	#[must_use]
	pub fn file_type(&self) -> FileType {
		FileType(self.0.file_type().unwrap())
	}

	#[must_use]
	pub fn len(&self) -> u64 {
		assert!(self.0.mask().intersects(StatxMask::Size));

		self.0.size
	}
}
