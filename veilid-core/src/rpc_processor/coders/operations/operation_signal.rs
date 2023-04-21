use super::*;

#[derive(Debug, Clone)]
pub struct RPCOperationSignal {
    signal_info: SignalInfo,
}

impl RPCOperationSignal {
    pub fn new(signal_info: SignalInfo) -> Self {
        Self { signal_info }
    }
    pub fn validate(&mut self, validate_context: &RPCValidateContext) -> Result<(), RPCError> {
        self.signal_info.validate(validate_context.crypto.clone())
    }
    pub fn decode(
        reader: &veilid_capnp::operation_signal::Reader,
    ) -> Result<RPCOperationSignal, RPCError> {
        let signal_info = decode_signal_info(reader)?;
        Ok(RPCOperationSignal { signal_info })
    }
    pub fn encode(
        &self,
        builder: &mut veilid_capnp::operation_signal::Builder,
    ) -> Result<(), RPCError> {
        encode_signal_info(&self.signal_info, builder)?;
        Ok(())
    }
    pub fn signal_info(&self) -> &SignalInfo {
        &self.signal_info
    }
    pub fn destructure(self) -> SignalInfo {
        self.signal_info
    }
}
