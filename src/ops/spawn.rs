use xx_core::coroutines;

use super::*;

#[asynchronous]
pub async fn spawn<T: Task + 'static>(task: T) -> coroutines::JoinHandle<T::Output> {
	let runtime = internal_get_runtime_context().await;

	unsafe { coroutines::spawn(runtime, task) }
}
