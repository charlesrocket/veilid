use crate::*;
use rpc_processor::*;

#[derive(Debug, Clone)]
pub enum RPCOperationKind {
    Question(RPCQuestion),
    Statement(RPCStatement),
    Answer(RPCAnswer),
}

impl RPCOperationKind {
    pub fn desc(&self) -> &'static str {
        match self {
            RPCOperationKind::Question(q) => q.desc(),
            RPCOperationKind::Statement(s) => s.desc(),
            RPCOperationKind::Answer(a) => a.desc(),
        }
    }

    pub fn decode(
        kind_reader: &veilid_capnp::operation::kind::Reader,
        opt_sender_node_id: Option<&DHTKey>,
    ) -> Result<Self, RPCError> {
        let which_reader = kind_reader.which().map_err(RPCError::protocol)?;
        let out = match which_reader {
            veilid_capnp::operation::kind::Which::Question(r) => {
                let q_reader = r.map_err(RPCError::protocol)?;
                let out = RPCQuestion::decode(&q_reader)?;
                RPCOperationKind::Question(out)
            }
            veilid_capnp::operation::kind::Which::Statement(r) => {
                let q_reader = r.map_err(RPCError::protocol)?;
                let out = RPCStatement::decode(&q_reader, opt_sender_node_id)?;
                RPCOperationKind::Statement(out)
            }
            veilid_capnp::operation::kind::Which::Answer(r) => {
                let q_reader = r.map_err(RPCError::protocol)?;
                let out = RPCAnswer::decode(&q_reader)?;
                RPCOperationKind::Answer(out)
            }
        };

        Ok(out)
    }

    pub fn encode(
        &self,
        builder: &mut veilid_capnp::operation::kind::Builder,
    ) -> Result<(), RPCError> {
        match self {
            RPCOperationKind::Question(k) => k.encode(&mut builder.reborrow().init_question()),
            RPCOperationKind::Statement(k) => k.encode(&mut builder.reborrow().init_statement()),
            RPCOperationKind::Answer(k) => k.encode(&mut builder.reborrow().init_answer()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RPCOperation {
    op_id: u64,
    sender_node_info: Option<SignedNodeInfo>,
    kind: RPCOperationKind,
}

impl RPCOperation {
    pub fn new_question(question: RPCQuestion, sender_node_info: Option<SignedNodeInfo>) -> Self {
        Self {
            op_id: intf::get_random_u64(),
            sender_node_info,
            kind: RPCOperationKind::Question(question),
        }
    }
    pub fn new_statement(
        statement: RPCStatement,
        sender_node_info: Option<SignedNodeInfo>,
    ) -> Self {
        Self {
            op_id: intf::get_random_u64(),
            sender_node_info,
            kind: RPCOperationKind::Statement(statement),
        }
    }

    pub fn new_answer(
        request: &RPCOperation,
        answer: RPCAnswer,
        sender_node_info: Option<SignedNodeInfo>,
    ) -> Self {
        Self {
            op_id: request.op_id,
            sender_node_info,
            kind: RPCOperationKind::Answer(answer),
        }
    }

    pub fn op_id(&self) -> u64 {
        self.op_id
    }

    pub fn sender_node_info(&self) -> Option<&SignedNodeInfo> {
        self.sender_node_info.as_ref()
    }

    pub fn kind(&self) -> &RPCOperationKind {
        &self.kind
    }

    pub fn into_kind(self) -> RPCOperationKind {
        self.kind
    }

    pub fn decode(
        operation_reader: &veilid_capnp::operation::Reader,
        opt_sender_node_id: Option<&DHTKey>,
    ) -> Result<Self, RPCError> {
        let op_id = operation_reader.get_op_id();

        let sender_node_info = if operation_reader.has_sender_node_info() {
            if let Some(sender_node_id) = opt_sender_node_id {
                let sni_reader = operation_reader
                    .get_sender_node_info()
                    .map_err(RPCError::protocol)?;
                let sni = decode_signed_node_info(&sni_reader, sender_node_id)?;
                Some(sni)
            } else {
                None
            }
        } else {
            None
        };

        let kind_reader = operation_reader.get_kind();
        let kind = RPCOperationKind::decode(&kind_reader, opt_sender_node_id)?;

        Ok(RPCOperation {
            op_id,
            sender_node_info,
            kind,
        })
    }

    pub fn encode(&self, builder: &mut veilid_capnp::operation::Builder) -> Result<(), RPCError> {
        builder.set_op_id(self.op_id);
        if let Some(sender_info) = &self.sender_node_info {
            let mut si_builder = builder.reborrow().init_sender_node_info();
            encode_signed_node_info(&sender_info, &mut si_builder)?;
        }
        let mut k_builder = builder.reborrow().init_kind();
        self.kind.encode(&mut k_builder)?;
        Ok(())
    }
}
