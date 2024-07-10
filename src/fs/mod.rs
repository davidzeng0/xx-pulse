use std::path::Path;

use xx_core::async_std::io::{ReadExt, SeekExt};
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

#[asynchronous]
#[allow(clippy::impl_trait_in_params, clippy::unwrap_used)]
pub async fn read_to_end(path: impl AsRef<Path>, vec: &mut Vec<u8>) -> Result<usize> {
	let mut file = File::open(path).await?;

	if let Ok(len) = file.stream_len().await {
		vec.reserve(len.try_into().unwrap());
	}

	file.read_to_end(vec).await
}

#[asynchronous]
#[allow(clippy::impl_trait_in_params)]
pub async fn read(path: impl AsRef<Path>) -> Result<Vec<u8>> {
	let mut vec = Vec::new();

	read_to_end(path, &mut vec).await?;

	Ok(vec)
}
