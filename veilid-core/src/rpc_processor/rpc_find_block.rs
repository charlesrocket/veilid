use super::*;

impl RPCProcessor {
    #[instrument(level = "trace", skip(self, msg), fields(msg.operation.op_id), ret, err)]
    pub(crate) async fn process_find_block_q(
        &self,
        msg: RPCMessage,
    ) -> Result<NetworkResult<()>, RPCError> {
        Err(RPCError::unimplemented("process_find_block_q"))
    }
}
