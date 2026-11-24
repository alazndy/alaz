//! Postcard variant order for tasks in formats through 32.

use crate::command::NodeId;
use crate::world::Task;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

// Each entry lists a variant shared by the two layouts. The two discarded
// variants keep their wire positions and decode as idle.
macro_rules! legacy_tasks {
    ($($before:ident $( { $($a:ident : $at:ty),* } )?,)* ;
     $($after:ident $( { $($b:ident : $bt:ty),* } )?,)*) => {
        #[derive(Serialize, Deserialize)]
        enum LegacyTask {
            $($before $( { $($a: $at),* } )?,)*
            Cave { node: NodeId, depth: u8, light: u16, act: u8, work: u16 },
            $($after $( { $($b: $bt),* } )?,)*
            Enter { node: NodeId },
        }

        impl From<Task> for LegacyTask {
            fn from(task: Task) -> Self {
                match task {
                    $(Task::$before $( { $($a),* } )? => Self::$before $( { $($a),* } )?,)*
                    $(Task::$after $( { $($b),* } )? => Self::$after $( { $($b),* } )?,)*
                }
            }
        }

        impl From<LegacyTask> for Task {
            fn from(task: LegacyTask) -> Self {
                match task {
                    $(LegacyTask::$before $( { $($a),* } )? => Self::$before $( { $($a),* } )?,)*
                    $(LegacyTask::$after $( { $($b),* } )? => Self::$after $( { $($b),* } )?,)*
                    LegacyTask::Cave { .. } | LegacyTask::Enter { .. } => Self::Idle,
                }
            }
        }
    };
}

legacy_tasks! {
    Idle,
    Move { x: i64, y: i64 },
    Gather { node: NodeId, progress: u32 },
    Drink { tx: i32, ty: i32 },
    Tame { wild: u32, progress: u32 },
    Hunt { wild: u32, progress: u32, chase: u32 },
    Craft { recipe: u16, step: u8, done: u8, hits: u8, idle: u16, salt: u16, score: u16, stack: [(i8, u8); 8], n: u8 },
    ;
    Walk { x: i64, y: i64 },
    Split { body: u32, progress: u32 },
    Pick { body: u32, progress: u32 },
    Fish { tx: i32, ty: i32, progress: u32 },
    Dig { tx: i32, ty: i32, progress: u32 },
}

pub(crate) fn serialize<S: Serializer>(task: &Task, serializer: S) -> Result<S::Ok, S::Error> {
    LegacyTask::from(*task).serialize(serializer)
}

pub(crate) fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Task, D::Error> {
    LegacyTask::deserialize(deserializer).map(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_cave_tasks_load_as_idle() {
        #[derive(Deserialize)]
        struct AgentTask(#[serde(with = "super")] Task);
        for task in [
            LegacyTask::Enter { node: NodeId(7) },
            LegacyTask::Cave {
                node: NodeId(7),
                depth: 1,
                light: 4,
                act: 2,
                work: 300,
            },
        ] {
            let bytes = postcard::to_allocvec(&task).unwrap();
            assert_eq!(
                postcard::from_bytes::<AgentTask>(&bytes).unwrap().0,
                Task::Idle
            );
        }
    }
}
