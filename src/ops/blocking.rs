use std::mem::{ManuallyDrop, MaybeUninit};

use xx_core::coroutines::block_on_thread_safe;
use xx_core::runtime::{catch_unwind_safe, join};
use xx_core::threadpool::*;

use super::*;

#[asynchronous]
pub async fn run_blocking<F, Output>(func: F) -> Result<Output>
where
	F: FnOnce(&WorkContext) -> Output + Send
{
	let driver = internal_get_driver().await;

	check_interrupt().await?;

	let mut output = MaybeUninit::uninit();
	let func = |context: &WorkContext| {
		output.write(catch_unwind_safe(|| func(context)));
	};

	let mut func = ManuallyDrop::new(func);

	/* Safety: unwinds are caught. the return value is checked */
	let mut work = unsafe { Work::new(&mut func) };

	/* Safety: we are blocked until the work finishes */
	let ran = block_on_thread_safe(unsafe { driver.run_work(ptr!(&mut work)) }).await;

	if ran {
		/* Safety: the value is initialized */
		Ok(join(unsafe { output.assume_init() }))
	} else {
		/* Safety: function not consumed by thread pool */
		unsafe { ManuallyDrop::drop(&mut func) };

		Err(ErrorKind::Interrupted.into())
	}
}
