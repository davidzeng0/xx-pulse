use super::*;

#[asynchronous]
pub async fn join<T1, T2>(task_1: T1, task_2: T2) -> Join<T1::Output, T2::Output>
where
	T1: Task,
	T2: Task
{
	let runtime = internal_get_runtime_context().await;

	/* Safety: runtimes and executor live until there are no more workers */
	unsafe { coroutines::join(runtime, task_1, task_2).await }
}

#[asynchronous]
pub async fn select<T1, T2>(task_1: T1, task_2: T2) -> Select<T1::Output, T2::Output>
where
	T1: Task,
	T2: Task
{
	let runtime = internal_get_runtime_context().await;

	/* Safety: runtimes and executor live until there are no more workers */
	unsafe { coroutines::select(runtime, task_1, task_2).await }
}

#[asynchronous]
pub async fn spawn<T>(task: T) -> JoinHandle<T::Output>
where
	T: Task + 'static
{
	let runtime = internal_get_runtime_context().await;

	/* Safety: task is static */
	unsafe { coroutines::spawn(runtime, task) }
}

pub mod internal {
	use super::*;

	#[asynchronous]
	pub async fn runtime() -> Ptr<Pulse> {
		internal_get_runtime_context().await
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
