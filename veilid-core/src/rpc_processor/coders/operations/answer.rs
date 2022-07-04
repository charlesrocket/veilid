use super::*;
use crate::*;
use rpc_processor::*;

#[derive(Debug, Clone)]
pub struct RPCAnswer {
    detail: RPCAnswerDetail,
}

impl RPCAnswer {
    pub fn new(detail: RPCAnswerDetail) -> Self {
        Self { detail }
    }
    pub fn detail(&self) -> &RPCAnswerDetail {
        &self.detail
    }
    pub fn desc(&self) -> &'static str {
        self.detail.desc()
    }
    pub fn decode(reader: &veilid_capnp::answer::Reader) -> Result<RPCAnswer, RPCError> {
        let d_reader = reader.get_detail();
        let detail = RPCAnswerDetail::decode(&d_reader)?;
        Ok(RPCAnswer { detail })
    }
    pub fn encode(&self, builder: &mut veilid_capnp::answer::Builder) -> Result<(), RPCError> {
        self.detail.encode(&mut builder.init_detail())?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum RPCAnswerDetail {
    StatusA(RPCOperationStatusA),
    FindNodeA(RPCOperationFindNodeA),
    GetValueA(RPCOperationGetValueA),
    SetValueA(RPCOperationSetValueA),
    WatchValueA(RPCOperationWatchValueA),
    SupplyBlockA(RPCOperationSupplyBlockA),
    FindBlockA(RPCOperationFindBlockA),
    StartTunnelA(RPCOperationStartTunnelA),
    CompleteTunnelA(RPCOperationCompleteTunnelA),
    CancelTunnelA(RPCOperationCancelTunnelA),
}

impl RPCAnswerDetail {
    pub fn desc(&self) -> &'static str {
        match self {
            RPCAnswerDetail::StatusA(_) => "StatusA",
            RPCAnswerDetail::FindNodeA(_) => "FindNodeA",
            RPCAnswerDetail::GetValueA(_) => "GetValueA",
            RPCAnswerDetail::SetValueA(_) => "SetValueA",
            RPCAnswerDetail::WatchValueA(_) => "WatchValueA",
            RPCAnswerDetail::SupplyBlockA(_) => "SupplyBlockA",
            RPCAnswerDetail::FindBlockA(_) => "FindBlockA",
            RPCAnswerDetail::StartTunnelA(_) => "StartTunnelA",
            RPCAnswerDetail::CompleteTunnelA(_) => "CompleteTunnelA",
            RPCAnswerDetail::CancelTunnelA(_) => "CancelTunnelA",
        }
    }

    pub fn decode(
        reader: &veilid_capnp::answer::detail::Reader,
    ) -> Result<RPCAnswerDetail, RPCError> {
        let which_reader = reader.which().map_err(map_error_capnp_notinschema!())?;
        let out = match which_reader {
            veilid_capnp::answer::detail::StatusA(r) => {
                let op_reader = r.map_err(map_error_capnp_notinschema!())?;
                let out = RPCOperationStatusA::decode(&op_reader)?;
                RPCAnswerDetail::StatusA(out)
            }
            veilid_capnp::answer::detail::FindNodeA(r) => {
                let op_reader = r.map_err(map_error_capnp_notinschema!())?;
                let out = RPCOperationFindNodeA::decode(&op_reader)?;
                RPCAnswerDetail::FindNodeA(out)
            }
            veilid_capnp::answer::detail::GetValueA(r) => {
                let op_reader = r.map_err(map_error_capnp_notinschema!())?;
                let out = RPCOperationGetValueA::decode(&op_reader)?;
                RPCAnswerDetail::GetValueA(out)
            }
            veilid_capnp::answer::detail::SetValueA(r) => {
                let op_reader = r.map_err(map_error_capnp_notinschema!())?;
                let out = RPCOperationSetValueA::decode(&op_reader)?;
                RPCAnswerDetail::SetValueA(out)
            }
            veilid_capnp::answer::detail::WatchValueA(r) => {
                let op_reader = r.map_err(map_error_capnp_notinschema!())?;
                let out = RPCOperationWatchValueA::decode(&op_reader)?;
                RPCAnswerDetail::WatchValueA(out)
            }
            veilid_capnp::answer::detail::SupplyBlockA(r) => {
                let op_reader = r.map_err(map_error_capnp_notinschema!())?;
                let out = RPCOperationSupplyBlockA::decode(&op_reader)?;
                RPCAnswerDetail::SupplyBlockA(out)
            }
            veilid_capnp::answer::detail::FindBlockA(r) => {
                let op_reader = r.map_err(map_error_capnp_notinschema!())?;
                let out = RPCOperationFindBlockA::decode(&op_reader)?;
                RPCAnswerDetail::FindBlockA(out)
            }
            veilid_capnp::answer::detail::StartTunnelA(r) => {
                let op_reader = r.map_err(map_error_capnp_notinschema!())?;
                let out = RPCOperationStartTunnelA::decode(&op_reader)?;
                RPCAnswerDetail::StartTunnelA(out)
            }
            veilid_capnp::answer::detail::CompleteTunnelA(r) => {
                let op_reader = r.map_err(map_error_capnp_notinschema!())?;
                let out = RPCOperationCompleteTunnelA::decode(&op_reader)?;
                RPCAnswerDetail::CompleteTunnelA(out)
            }
            veilid_capnp::answer::detail::CancelTunnelA(r) => {
                let op_reader = r.map_err(map_error_capnp_notinschema!())?;
                let out = RPCOperationCancelTunnelA::decode(&op_reader)?;
                RPCAnswerDetail::CancelTunnelA(out)
            }
        };
        Ok(out)
    }
    pub fn encode(
        &self,
        builder: &mut veilid_capnp::answer::detail::Builder,
    ) -> Result<(), RPCError> {
        match self {
            RPCAnswerDetail::StatusA(d) => d.encode(&mut builder.init_status_a()),
            RPCAnswerDetail::FindNodeA(d) => d.encode(&mut builder.init_find_node_a()),
            RPCAnswerDetail::GetValueA(d) => d.encode(&mut builder.init_get_value_a()),
            RPCAnswerDetail::SetValueA(d) => d.encode(&mut builder.init_set_value_a()),
            RPCAnswerDetail::WatchValueA(d) => d.encode(&mut builder.init_watch_value_a()),
            RPCAnswerDetail::SupplyBlockA(d) => d.encode(&mut builder.init_supply_block_a()),
            RPCAnswerDetail::FindBlockA(d) => d.encode(&mut builder.init_find_block_a()),
            RPCAnswerDetail::StartTunnelA(d) => d.encode(&mut builder.init_start_tunnel_a()),
            RPCAnswerDetail::CompleteTunnelA(d) => d.encode(&mut builder.init_complete_tunnel_a()),
            RPCAnswerDetail::CancelTunnelA(d) => d.encode(&mut builder.init_cancel_tunnel_a()),
        }
    }
}
