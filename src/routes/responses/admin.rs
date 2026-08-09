use crate::db::entities::{programs::Model as ProgramModel, rpcs::Model as RpcModel};
use serde::Serialize;

#[derive(Serialize)]
pub struct RpcWithProgramsResponse {
    #[serde(flatten)]
    pub rpc: RpcModel,

    pub programs: Vec<ProgramModel>,
}

#[derive(Serialize)]
pub struct GetRpcsResponse {
    pub data: Vec<RpcWithProgramsResponse>,
    pub total: u64,
    pub pages: u64,
}

impl GetRpcsResponse {
    pub fn new(rpcs: Vec<(RpcModel, Vec<ProgramModel>)>, total: u64, pages: u64) -> Self {
        Self {
            total,
            pages,
            data: rpcs
                .into_iter()
                .map(|(rpc, programs)| RpcWithProgramsResponse { rpc, programs })
                .collect(),
        }
    }
}

#[macro_export]
macro_rules! unknown_rpc {
    () => {{
        let now = chrono::Utc::now().naive_utc();
        RpcModel {
            id: 0,
            http_url: "UNKNOWN".to_owned(),
            ws_url: "UNKNOWN".to_owned(),
            created_at: now,
            updated_at: now,
        }
    }};
}

#[derive(Serialize)]
pub struct ProgramWithRpcResponse {
    #[serde(flatten)]
    pub program: ProgramModel,

    pub rpc: RpcModel,
}

impl ProgramWithRpcResponse {
    pub fn new_no_rpc(program: ProgramModel) -> Self {
        Self {
            program,
            rpc: unknown_rpc!(), // The reason for not using Option, is that this is a very rare case
        } // It's not worth checking nullness of rpc everytime on each action!
    }
}
#[derive(Serialize)]
pub struct GetProgramsResponse {
    pub data: Vec<ProgramWithRpcResponse>,
    pub total: u64,
    pub pages: u64,
}

impl GetProgramsResponse {
    pub fn new(programs: Vec<(ProgramModel, Option<RpcModel>)>, total: u64, pages: u64) -> Self {
        Self {
            total,
            pages,
            data: programs
                .into_iter()
                .map(|(program, rpc)| ProgramWithRpcResponse {
                    program,
                    rpc: rpc.unwrap_or(unknown_rpc!()),
                })
                .collect(),
        }
    }
}
