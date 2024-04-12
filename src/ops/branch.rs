use super::*;

#[asynchronous]
#[allow(clippy::multiple_unsafe_ops_per_block)]
pub(crate) async fn spawn_entry<T, Output>(task: T) -> Output
where
	T: for<'a> Task<Output<'a> = Output>
{
	let workers = internal_get_pulse_env().await.workers;

	/* Safety: the worker is appended to the list */
	let worker = unsafe { PulseWorker::new().await };

	/* Safety: worker is pinned */
	unsafe { ptr!(workers=>append(&worker.node)) };

	task.await
}

#[asynchronous]
pub async fn join<T1, T2, O1, O2>(task_1: T1, task_2: T2) -> Join<O1, O2>
where
	T1: for<'a> Task<Output<'a> = O1>,
	T2: for<'a> Task<Output<'a> = O2>
{
	let runtime = internal_get_pulse_env().await;

	/* Safety: runtimes and executor live until there are no more workers */
	unsafe { coroutines::join(runtime, task_1, task_2).await }
}

#[asynchronous]
pub async fn select<T1, T2, O1, O2>(task_1: T1, task_2: T2) -> Select<O1, O2>
where
	T1: for<'a> Task<Output<'a> = O1>,
	T2: for<'a> Task<Output<'a> = O2>
{
	let runtime = internal_get_pulse_env().await;

	/* Safety: runtimes and executor live until there are no more workers */
	unsafe { coroutines::select(runtime, task_1, task_2).await }
}

#[asynchronous]
pub async fn spawn<T, Output>(task: T) -> JoinHandle<Output>
where
	T: for<'a> Task<Output<'a> = Output> + 'static
{
	let runtime = internal_get_pulse_env().await;

	/* Safety: task is static */
	unsafe { coroutines::spawn(runtime, spawn_entry(task)) }
}

pub mod internal {
	use super::*;

	#[asynchronous]
	#[context('current)]
	pub async unsafe fn runtime() -> &'current Pulse {
		internal_get_pulse_env().await
	}
}

#[macro_export]
macro_rules! select_many {
	{$($tokens:tt)*} => {
		#[allow(clippy::multiple_unsafe_ops_per_block)]
		/* Safety: runtimes and executor live until there are no more workers */
		unsafe {
			::xx_core::coroutines::select! {
				$crate::ops::branch::internal::runtime().await;
				$($tokens)*
			}
		}
	}
}

pub use select_many;

#[macro_export]
macro_rules! join_many {
	($($tokens:tt)*) => {
		#[allow(clippy::multiple_unsafe_ops_per_block)]
		/* Safety: runtimes and executor live until there are no more workers */
		unsafe {
			::xx_core::coroutines::join! {
				$crate::ops::branch::internal::runtime().await;
				$($tokens)*
			}
		}
	}
}

pub use join_many;
