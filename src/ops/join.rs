use xx_core::coroutines;

use super::*;

#[async_fn]
pub async fn join<T1: Task, T2: Task>(
	task_1: T1, task_2: T2
) -> coroutines::Join<T1::Output, T2::Output> {
	let runtime = internal_get_runtime_context().await;

	unsafe { coroutines::join(runtime, task_1, task_2).await }
}
