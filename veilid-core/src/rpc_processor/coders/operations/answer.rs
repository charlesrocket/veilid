use super::*;

#[derive(Debug, Clone)]
pub struct RPCAnswer {
    detail: RPCAnswerDetail,
}

impl RPCAnswer {
    pub fn new(detail: RPCAnswerDetail) -> Self {
        Self { detail }
    }
    pub fn validate(&mut self, validate_context: &RPCValidateContext) -> Result<(), RPCError> {
        self.detail.validate(validate_context)
    }
    pub fn desc(&self) -> &'static str {
        self.detail.desc()
    }
    pub fn destructure(self) -> RPCAnswerDetail {
        self.detail
    }
    pub fn decode(reader: &veilid_capnp::answer::Reader) -> Result<RPCAnswer, RPCError> {
        let d_reader = reader.get_detail();
        let detail = RPCAnswerDetail::decode(&d_reader)?;
        Ok(RPCAnswer { detail })
    }
    pub fn encode(&self, builder: &mut veilid_capnp::answer::Builder) -> Result<(), RPCError> {
        self.detail.encode(&mut builder.reborrow().init_detail())?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum RPCAnswerDetail {
    StatusA(RPCOperationStatusA),
    FindNodeA(RPCOperationFindNodeA),
    AppCallA(RPCOperationAppCallA),
    GetValueA(RPCOperationGetValueA),
    SetValueA(RPCOperationSetValueA),
    WatchValueA(RPCOperationWatchValueA),
    SupplyBlockA(RPCOperationSupplyBlockA),
    FindBlockA(RPCOperationFindBlockA),
    #[cfg(feature = "unstable-tunnels")]
    StartTunnelA(RPCOperationStartTunnelA),
    #[cfg(feature = "unstable-tunnels")]
    CompleteTunnelA(RPCOperationCompleteTunnelA),
    #[cfg(feature = "unstable-tunnels")]
    CancelTunnelA(RPCOperationCancelTunnelA),
}

impl RPCAnswerDetail {
    pub fn desc(&self) -> &'static str {
        match self {
            RPCAnswerDetail::StatusA(_) => "StatusA",
            RPCAnswerDetail::FindNodeA(_) => "FindNodeA",
            RPCAnswerDetail::AppCallA(_) => "AppCallA",
            RPCAnswerDetail::GetValueA(_) => "GetValueA",
            RPCAnswerDetail::SetValueA(_) => "SetValueA",
            RPCAnswerDetail::WatchValueA(_) => "WatchValueA",
            RPCAnswerDetail::SupplyBlockA(_) => "SupplyBlockA",
            RPCAnswerDetail::FindBlockA(_) => "FindBlockA",
            #[cfg(feature = "unstable-tunnels")]
            RPCAnswerDetail::StartTunnelA(_) => "StartTunnelA",
            #[cfg(feature = "unstable-tunnels")]
            RPCAnswerDetail::CompleteTunnelA(_) => "CompleteTunnelA",
            #[cfg(feature = "unstable-tunnels")]
            RPCAnswerDetail::CancelTunnelA(_) => "CancelTunnelA",
        }
    }
    pub fn validate(&mut self, validate_context: &RPCValidateContext) -> Result<(), RPCError> {
        match self {
            RPCAnswerDetail::StatusA(r) => r.validate(validate_context),
            RPCAnswerDetail::FindNodeA(r) => r.validate(validate_context),
            RPCAnswerDetail::AppCallA(r) => r.validate(validate_context),
            RPCAnswerDetail::GetValueA(r) => r.validate(validate_context),
            RPCAnswerDetail::SetValueA(r) => r.validate(validate_context),
            RPCAnswerDetail::WatchValueA(r) => r.validate(validate_context),
            RPCAnswerDetail::SupplyBlockA(r) => r.validate(validate_context),
            RPCAnswerDetail::FindBlockA(r) => r.validate(validate_context),
            #[cfg(feature = "unstable-tunnels")]
            RPCAnswerDetail::StartTunnelA(r) => r.validate(validate_context),
            #[cfg(feature = "unstable-tunnels")]
            RPCAnswerDetail::CompleteTunnelA(r) => r.validate(validate_context),
            #[cfg(feature = "unstable-tunnels")]
            RPCAnswerDetail::CancelTunnelA(r) => r.validate(validate_context),
        }
    }
    pub fn decode(
        reader: &veilid_capnp::answer::detail::Reader,
    ) -> Result<RPCAnswerDetail, RPCError> {
        let which_reader = reader.which().map_err(RPCError::protocol)?;
        let out = match which_reader {
            veilid_capnp::answer::detail::StatusA(r) => {
                let op_reader = r.map_err(RPCError::protocol)?;
                let out = RPCOperationStatusA::decode(&op_reader)?;
                RPCAnswerDetail::StatusA(out)
            }
            veilid_capnp::answer::detail::FindNodeA(r) => {
                let op_reader = r.map_err(RPCError::protocol)?;
                let out = RPCOperationFindNodeA::decode(&op_reader)?;
                RPCAnswerDetail::FindNodeA(out)
            }
            veilid_capnp::answer::detail::AppCallA(r) => {
                let op_reader = r.map_err(RPCError::protocol)?;
                let out = RPCOperationAppCallA::decode(&op_reader)?;
                RPCAnswerDetail::AppCallA(out)
            }
            veilid_capnp::answer::detail::GetValueA(r) => {
                let op_reader = r.map_err(RPCError::protocol)?;
                let out = RPCOperationGetValueA::decode(&op_reader)?;
                RPCAnswerDetail::GetValueA(out)
            }
            veilid_capnp::answer::detail::SetValueA(r) => {
                let op_reader = r.map_err(RPCError::protocol)?;
                let out = RPCOperationSetValueA::decode(&op_reader)?;
                RPCAnswerDetail::SetValueA(out)
            }
            veilid_capnp::answer::detail::WatchValueA(r) => {
                let op_reader = r.map_err(RPCError::protocol)?;
                let out = RPCOperationWatchValueA::decode(&op_reader)?;
                RPCAnswerDetail::WatchValueA(out)
            }
            veilid_capnp::answer::detail::SupplyBlockA(r) => {
                let op_reader = r.map_err(RPCError::protocol)?;
                let out = RPCOperationSupplyBlockA::decode(&op_reader)?;
                RPCAnswerDetail::SupplyBlockA(out)
            }
            veilid_capnp::answer::detail::FindBlockA(r) => {
                let op_reader = r.map_err(RPCError::protocol)?;
                let out = RPCOperationFindBlockA::decode(&op_reader)?;
                RPCAnswerDetail::FindBlockA(out)
            }
            #[cfg(feature = "unstable-tunnels")]
            veilid_capnp::answer::detail::StartTunnelA(r) => {
                let op_reader = r.map_err(RPCError::protocol)?;
                let out = RPCOperationStartTunnelA::decode(&op_reader)?;
                RPCAnswerDetail::StartTunnelA(out)
            }
            #[cfg(feature = "unstable-tunnels")]
            veilid_capnp::answer::detail::CompleteTunnelA(r) => {
                let op_reader = r.map_err(RPCError::protocol)?;
                let out = RPCOperationCompleteTunnelA::decode(&op_reader)?;
                RPCAnswerDetail::CompleteTunnelA(out)
            }
            #[cfg(feature = "unstable-tunnels")]
            veilid_capnp::answer::detail::CancelTunnelA(r) => {
                let op_reader = r.map_err(RPCError::protocol)?;
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
            RPCAnswerDetail::StatusA(d) => d.encode(&mut builder.reborrow().init_status_a()),
            RPCAnswerDetail::FindNodeA(d) => d.encode(&mut builder.reborrow().init_find_node_a()),
            RPCAnswerDetail::AppCallA(d) => d.encode(&mut builder.reborrow().init_app_call_a()),
            RPCAnswerDetail::GetValueA(d) => d.encode(&mut builder.reborrow().init_get_value_a()),
            RPCAnswerDetail::SetValueA(d) => d.encode(&mut builder.reborrow().init_set_value_a()),
            RPCAnswerDetail::WatchValueA(d) => {
                d.encode(&mut builder.reborrow().init_watch_value_a())
            }
            RPCAnswerDetail::SupplyBlockA(d) => {
                d.encode(&mut builder.reborrow().init_supply_block_a())
            }
            RPCAnswerDetail::FindBlockA(d) => d.encode(&mut builder.reborrow().init_find_block_a()),
            #[cfg(feature = "unstable-tunnels")]
            RPCAnswerDetail::StartTunnelA(d) => {
                d.encode(&mut builder.reborrow().init_start_tunnel_a())
            }
            #[cfg(feature = "unstable-tunnels")]
            RPCAnswerDetail::CompleteTunnelA(d) => {
                d.encode(&mut builder.reborrow().init_complete_tunnel_a())
            }
            #[cfg(feature = "unstable-tunnels")]
            RPCAnswerDetail::CancelTunnelA(d) => {
                d.encode(&mut builder.reborrow().init_cancel_tunnel_a())
            }
        }
    }
}
