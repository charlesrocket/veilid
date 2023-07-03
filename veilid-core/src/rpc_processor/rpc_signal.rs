use super::*;

impl RPCProcessor {
    // Sends a unidirectional signal to a node
    // Can be sent via relays but not routes. For routed 'signal' like capabilities, use AppMessage.
    #[cfg_attr(
        feature = "verbose-tracing",
        instrument(level = "trace", skip(self), ret, err)
    )]
    pub async fn rpc_call_signal(
        self,
        dest: Destination,
        signal_info: SignalInfo,
    ) -> Result<NetworkResult<()>, RPCError> {
        // Ensure destination never has a private route
        if matches!(
            dest,
            Destination::PrivateRoute {
                private_route: _,
                safety_selection: _
            }
        ) {
            return Err(RPCError::internal(
                "Never send signal requests over private routes",
            ));
        }

        let signal = RPCOperationSignal::new(signal_info);
        let statement = RPCStatement::new(RPCStatementDetail::Signal(signal));

        // Send the signal request
        self.statement(dest, statement).await
    }

    #[cfg_attr(feature="verbose-tracing", instrument(level = "trace", skip(self, msg), fields(msg.operation.op_id), ret, err))]
    pub(crate) async fn process_signal(
        &self,
        msg: RPCMessage,
    ) -> Result<NetworkResult<()>, RPCError> {
        // Ignore if disabled
        {
            let c = self.config.get();
            if c.capabilities.disable.contains(&CAP_WILL_SIGNAL) {
                return Ok(NetworkResult::service_unavailable("signal is disabled"));
            }
        }

        // Can't allow anything other than direct packets here, as handling reverse connections
        // or anything like via signals over private routes would deanonymize the route
        match &msg.header.detail {
            RPCMessageHeaderDetail::Direct(_) => {}
            RPCMessageHeaderDetail::SafetyRouted(_) | RPCMessageHeaderDetail::PrivateRouted(_) => {
                return Ok(NetworkResult::invalid_message("signal must be direct"));
            }
        };

        // Get the statement
        let (_, _, _, kind) = msg.operation.destructure();
        let signal = match kind {
            RPCOperationKind::Statement(s) => match s.destructure() {
                RPCStatementDetail::Signal(s) => s,
                _ => panic!("not a signal"),
            },
            _ => panic!("not a statement"),
        };

        // Handle it
        let network_manager = self.network_manager();
        let signal_info = signal.destructure();
        network_manager
            .handle_signal(signal_info)
            .await
            .map_err(RPCError::network)
    }
}
