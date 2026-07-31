use crate::db::entities::{programs::Model as ProgramModel, rpcs::Model as RpcModel};
use serde::Serialize;

#[derive(Serialize)]
pub struct RpcWithProgramsResponse {
    #[serde(flatten)]
    rpc: RpcModel,

    programs: Vec<ProgramModel>,
}

#[derive(Serialize)]
pub struct GetRpcsResponse {
    data: Vec<RpcWithProgramsResponse>,
    total: u16,
    pages: u16,
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
