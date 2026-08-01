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
    pub total: u16,
    pub pages: u16,
}

impl GetRpcsResponse {
    pub fn new(rpcs: Vec<(RpcModel, Vec<ProgramModel>)>, total: u16, pages: u16) -> Self {
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


#[derive(Serialize)]
pub struct ProgramWithRpcResponse {
    #[serde(flatten)]
    pub program: ProgramModel,

    pub rpc: RpcModel,
}