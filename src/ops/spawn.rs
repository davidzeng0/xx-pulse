use xx_core::coroutines;

use super::*;

#[async_fn]
pub async fn spawn<T: Task + 'static>(task: T) -> coroutines::JoinHandle<T::Output> {
	let runtime = internal_get_runtime_context().await;

	coroutines::spawn(runtime, task).await
}
