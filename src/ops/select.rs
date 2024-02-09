use xx_core::coroutines;

use super::*;

#[asynchronous]
pub async fn select<T1: Task, T2: Task>(
	task_1: T1, task_2: T2
) -> coroutines::Select<T1::Output, T2::Output> {
	let runtime = internal_get_runtime_context().await;

	unsafe { coroutines::select(runtime, task_1, task_2).await }
}
