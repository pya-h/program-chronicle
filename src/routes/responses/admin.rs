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
}

impl GetRpcsResponse {
    pub fn new(rpcs: Vec<(RpcModel, Vec<ProgramModel>)>) -> Self {
        let total = rpcs.len() as u16;
        Self {
            total,
            data: rpcs
                .into_iter()
                .map(|(rpc, programs)| RpcWithProgramsResponse { rpc, programs })
                .collect(),
        }
    }
}
