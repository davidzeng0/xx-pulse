use std::ffi::OsStr;
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use enumflags2::make_bitflags;
use xx_core::async_std::AsyncIterator;
use xx_core::error::*;
use xx_core::os::dirent::{DirEnts, DirentDef};
use xx_core::os::error::OsError;
use xx_core::os::fcntl::*;
use xx_core::os::stat::*;

use super::*;

struct Dir {
	path: PathBuf,
	fd: OwnedFd
}

pub struct DirEntry {
	dir: Arc<Dir>,
	ent: DirentDef<PathBuf>
}

#[asynchronous]
impl DirEntry {
	#[must_use]
	pub fn path(&self) -> PathBuf {
		self.dir.path.join(self.file_name())
	}

	#[must_use]
	pub fn file_name(&self) -> &OsStr {
		self.ent.name.as_os_str()
	}

	#[must_use]
	pub const fn ino(&self) -> u64 {
		self.ent.ino
	}

	pub async fn metadata(&self) -> Result<Metadata> {
		let mut statx = Statx::default();

		io::statx(
			Some(self.dir.fd.as_fd()),
			self.file_name(),
			BitFlags::default(),
			BitFlags::default(),
			&mut statx
		)
		.await?;

		Ok(Metadata(statx))
	}

	#[allow(clippy::unwrap_used, clippy::missing_panics_doc)]
	#[must_use]
	pub fn file_type(&self) -> FileType {
		FileType(self.ent.file_type().unwrap())
	}
}

pub struct ReadDir {
	dir: Arc<Dir>,
	entries: DirEnts
}

#[asynchronous]
impl ReadDir {
	async fn next(&mut self) -> Result<Option<DirEntry>> {
		while !self.entries.is_eof() {
			if !self.entries.has_next_cached() {
				run_blocking(|_| self.entries.read_from_fd(self.dir.fd.as_fd())).await??;

				continue;
			}

			#[allow(clippy::unwrap_used)]
			let entry = self.entries.next_entry().unwrap();

			if entry.name == c"." || entry.name == c".." {
				continue;
			}

			let name = OsStr::from_bytes(entry.name.to_bytes())
				.to_os_string()
				.into();

			let ent = DirEntry {
				dir: self.dir.clone(),
				ent: DirentDef {
					ino: entry.ino,
					off: entry.off,
					reclen: entry.reclen,
					ty: entry.ty,
					name
				}
			};

			return Ok(Some(ent));
		}

		Ok(None)
	}
}

#[asynchronous]
impl AsyncIterator for ReadDir {
	type Item = Result<DirEntry>;

	async fn next(&mut self) -> Option<Self::Item> {
		self.next().await.transpose()
	}
}

#[asynchronous]
#[allow(clippy::impl_trait_in_params)]
pub async fn read_dir(path: impl AsRef<Path> + Send) -> Result<ReadDir> {
	let path = path.as_ref();
	let flags = make_bitflags!(OpenFlag::{
		Directory | LargeFile | CloseOnExec | NonBlock
	});

	let fd = io::open(path, flags, 0).await?;
	let mut statx = Statx::default();

	io::statx_fd(
		fd.as_fd(),
		BitFlags::default(),
		BitFlags::default(),
		&mut statx
	)
	.await?;

	if statx.file_type() != Some(dirent::FileType::Directory) {
		return Err(OsError::NotDir.into());
	}

	let entries = DirEnts::new_from_block_size(statx.block_size as usize);

	Ok(ReadDir {
		dir: Arc::new(Dir { path: path.to_owned(), fd }),
		entries
	})
}
