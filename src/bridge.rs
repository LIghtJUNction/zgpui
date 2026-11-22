//! Portable, bounded frontend state for an explicitly connected zbridge host.
//!
//! The caller owns its transport, connection lease and request IDs. Each call
//! invokes that transport once. A response receipt is not proof that the native
//! task finished, and an interrupted command is never replayed by this model.

use alloc::vec::Vec;
use core::future::Future;

/// The shared protocol types and bounded codecs used by this adapter.
pub use zbridge as protocol;
use zbridge::{
    BridgeError, Capabilities, CommandResult, Effect, ErrorCode, EventPage, Id, MAX_FRAME_BYTES,
    Mode, Operation, Request, Response, ResponseBody, State,
};

#[derive(Clone, Debug)]
pub enum CommandOutcome {
    AwaitingReply,
    Receipt(CommandResult),
    Error(BridgeError),
}

/// Kept with the original request and generation, including after reconnect.
#[derive(Clone, Debug)]
pub struct CommandRecord {
    pub request_id: Id,
    pub generation: Id,
    pub command: Id,
    pub outcome: CommandOutcome,
}

impl CommandRecord {
    pub fn is_uncertain(&self) -> bool {
        matches!(&self.outcome, CommandOutcome::Error(error) if error.effect == Effect::MayHaveOccurred)
    }
}

/// Retains one current capability set, state, event page and command receipt.
/// Every value is checked against zbridge's byte limits before it is retained.
/// It creates no runtime, listener, process or persistent history.
#[derive(Default)]
pub struct BridgeModel {
    generation: Option<Id>,
    capabilities: Option<Capabilities>,
    state: Option<State>,
    events: Option<EventPage>,
    command: Option<CommandRecord>,
    error: Option<BridgeError>,
}

impl BridgeModel {
    pub fn generation(&self) -> Option<&Id> {
        self.generation.as_ref()
    }
    pub fn capabilities(&self) -> Option<&Capabilities> {
        self.capabilities.as_ref()
    }
    pub fn state(&self) -> Option<&State> {
        self.state.as_ref()
    }
    pub fn events(&self) -> Option<&EventPage> {
        self.events.as_ref()
    }
    pub fn last_command(&self) -> Option<&CommandRecord> {
        self.command.as_ref()
    }
    pub fn last_error(&self) -> Option<&BridgeError> {
        self.error.as_ref()
    }

    /// Clear live projections without rebinding the previous command receipt.
    /// Discover capabilities explicitly after reconnecting.
    pub fn disconnect(&mut self) {
        self.clear_live();
    }

    fn clear_live(&mut self) {
        self.generation = None;
        self.capabilities = None;
        self.state = None;
        self.events = None;
    }

    fn preflight(&self, request: &Request) -> Result<(), BridgeError> {
        request.validate()?;
        let max_bytes = if matches!(&request.operation, Operation::Capabilities {}) {
            MAX_FRAME_BYTES
        } else {
            let capabilities = self.capabilities.as_ref().ok_or_else(|| {
                BridgeError::new(ErrorCode::Unavailable, "discover bridge capabilities first")
            })?;
            if request.operation.generation() != self.generation.as_ref() {
                return Err(BridgeError::new(
                    ErrorCode::StaleGeneration,
                    "request targets a different bridge generation",
                ));
            }
            match &request.operation {
                Operation::State { resource, .. } => {
                    if !capabilities
                        .resources
                        .iter()
                        .any(|item| item.id == *resource)
                    {
                        return Err(BridgeError::new(
                            ErrorCode::Unavailable,
                            "state resource is not advertised",
                        ));
                    }
                }
                Operation::Events { .. } if !capabilities.events => {
                    return Err(BridgeError::new(
                        ErrorCode::Unavailable,
                        "events are not advertised",
                    ));
                }
                Operation::Command { command, .. }
                    if capabilities.mode != Mode::Control
                        || !capabilities.commands.iter().any(|item| item.id == *command) =>
                {
                    return Err(BridgeError::new(
                        ErrorCode::Denied,
                        "command is not advertised for this frontend",
                    ));
                }
                _ => {}
            }
            capabilities.max_frame_bytes as usize
        };
        zbridge::encoded_len(request, max_bytes)?;
        Ok(())
    }

    /// Exchange one typed request with an explicit transport callback.
    ///
    /// The host must still authorize the request. Local capability checks only
    /// help the frontend avoid presenting unavailable actions. Transport errors,
    /// invalid replies and a dropped future leave sent command effects unknown.
    pub async fn request_with<F, Fut>(
        &mut self,
        request: Request,
        transport: F,
    ) -> Result<ResponseBody, BridgeError>
    where
        F: FnOnce(Request) -> Fut,
        Fut: Future<Output = Result<Response, BridgeError>>,
    {
        if let Err(error) = self.preflight(&request) {
            self.error = Some(error.clone());
            return Err(error);
        }
        self.error = None;
        if let Operation::Command {
            generation,
            command,
            ..
        } = &request.operation
        {
            self.command = Some(CommandRecord {
                request_id: request.id.clone(),
                generation: generation.clone(),
                command: command.clone(),
                outcome: CommandOutcome::AwaitingReply,
            });
        }
        let mut flight = Flight {
            model: self,
            request: request.clone(),
            settled: false,
        };
        let response = transport(request).await;
        let result = flight.model.settle(&flight.request, response);
        flight.settled = true;
        result
    }

    /// Adapt a byte transport through the same shared zbridge codecs.
    /// The transport must bound reads before allocating a complete frame.
    pub async fn request_json_with<F, Fut>(
        &mut self,
        request: Request,
        transport: F,
    ) -> Result<ResponseBody, BridgeError>
    where
        F: FnOnce(Vec<u8>) -> Fut,
        Fut: Future<Output = Result<Vec<u8>, BridgeError>>,
    {
        self.request_with(request, |request| async move {
            let encoded = zbridge::encode_request(&request)?;
            let response = transport(encoded).await?;
            zbridge::decode_response(&response)
        })
        .await
    }

    fn settle(
        &mut self,
        request: &Request,
        response: Result<Response, BridgeError>,
    ) -> Result<ResponseBody, BridgeError> {
        let response = match response.and_then(|response| {
            response.validate_for(request)?;
            if !matches!(&request.operation, Operation::Capabilities {}) {
                let capabilities = self.capabilities.as_ref().ok_or_else(|| {
                    BridgeError::new(ErrorCode::Closed, "bridge capabilities are unavailable")
                })?;
                zbridge::encoded_len(&response, capabilities.max_frame_bytes as usize)?;
                if let ResponseBody::Events(page) = &response.body {
                    zbridge::encoded_len(page, capabilities.max_event_page_bytes as usize)?;
                }
                if let ResponseBody::State(state) = &response.body
                    && !capabilities.resources.iter().any(|item| {
                        item.id == state.resource && item.schema_version == state.schema_version
                    })
                {
                    return Err(BridgeError::new(
                        ErrorCode::Conflict,
                        "state schema changed; refresh capabilities",
                    ));
                }
            }
            Ok(response)
        }) {
            Ok(response) => response,
            Err(error) => return Err(self.interrupted(request, error)),
        };
        if let ResponseBody::Error(error) = response.body {
            if request
                .operation
                .generation()
                .is_some_and(|expected| response.generation.as_ref() != Some(expected))
                || matches!(
                    error.code,
                    ErrorCode::Closed | ErrorCode::Denied | ErrorCode::StaleGeneration
                )
            {
                self.clear_live();
            }
            if error.effect == Effect::MayHaveOccurred {
                self.state = None;
            }
            self.record_error(request, &error);
            return Err(error);
        }
        if let ResponseBody::Capabilities(capabilities) = &response.body {
            // Even within one generation, advertised schemas and access can
            // change. An explicit refresh starts a fresh live projection.
            self.state = None;
            self.events = None;
            self.generation = response.generation.clone();
            self.capabilities = Some(capabilities.clone());
        } else if let ResponseBody::State(state) = &response.body {
            self.state = Some(state.clone());
        } else if let ResponseBody::Events(page) = &response.body {
            if page.retention_gap {
                self.state = None;
            }
            self.events = Some(page.clone());
        } else if let ResponseBody::Command(receipt) = &response.body {
            self.state = None;
            if let Some(record) = self.command.as_mut() {
                record.outcome = CommandOutcome::Receipt(receipt.clone());
            }
        }
        Ok(response.body)
    }

    fn record_error(&mut self, request: &Request, error: &BridgeError) {
        if request.operation.is_command()
            && let Some(record) = self
                .command
                .as_mut()
                .filter(|record| record.request_id == request.id)
        {
            record.outcome = CommandOutcome::Error(error.clone());
        }
        self.error = Some(error.clone());
    }

    fn interrupted(&mut self, request: &Request, error: BridgeError) -> BridgeError {
        // A typed callback can construct BridgeError directly. Normalize its
        // message before retention; only sent commands can have side effects.
        let error = BridgeError::new(error.code, error.message);
        let error = if request.operation.is_command() {
            error.uncertain()
        } else {
            error
        };
        self.record_error(request, &error);
        self.clear_live();
        error
    }
}

struct Flight<'a> {
    model: &'a mut BridgeModel,
    request: Request,
    settled: bool,
}

impl Drop for Flight<'_> {
    fn drop(&mut self) {
        if !self.settled {
            self.model.interrupted(
                &self.request,
                BridgeError::new(
                    ErrorCode::Unavailable,
                    "bridge exchange was interrupted before a reply was accepted",
                ),
            );
        }
    }
}

#[cfg(test)]
mod tests;
