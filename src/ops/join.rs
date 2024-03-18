use super::*;

#[asynchronous]
pub async fn join<T1, T2>(task_1: T1, task_2: T2) -> Join<T1::Output, T2::Output>
where
	T1: Task,
	T2: Task
{
	let runtime = internal_get_runtime_context().await;

	coroutines::join(runtime, task_1, task_2).await
}
