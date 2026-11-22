use super::*;
use alloc::{boxed::Box, string::ToString, vec};
use serde_json::{Value, json};
use std::{
    cell::Cell,
    future::{Future, pending},
    sync::atomic::{AtomicUsize, Ordering},
    task::{Context, Poll, Waker},
};
use zbridge::{
    Access, Command, Counter, Event, Host, HostFuture, MAX_PAGE_BYTES, Resource, VERSION,
};

fn id(value: &str) -> Id {
    Id::new(value).unwrap()
}

fn ready<T>(future: impl Future<Output = T>) -> T {
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    let mut future = std::pin::pin!(future);
    match future.as_mut().poll(&mut context) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("fixture unexpectedly waited"),
    }
}

struct Fixture {
    control: bool,
    generation: &'static str,
    writes: AtomicUsize,
}

impl Fixture {
    fn new(control: bool) -> Self {
        Self {
            control,
            generation: "generation-1",
            writes: AtomicUsize::new(0),
        }
    }
}

impl Host for Fixture {
    fn authorize(&self, access: Access) -> Result<(), BridgeError> {
        if access == Access::Control && !self.control {
            Err(BridgeError::new(ErrorCode::Denied, "observer fixture"))
        } else {
            Ok(())
        }
    }

    fn generation(&self) -> Result<Id, BridgeError> {
        Id::new(self.generation)
    }

    fn capabilities(&self) -> Result<Capabilities, BridgeError> {
        Ok(Capabilities {
            application: id("fixture"),
            mode: if self.control {
                Mode::Control
            } else {
                Mode::Observe
            },
            resources: vec![Resource {
                id: id("status"),
                schema_version: 1,
                description: "Current fixture state".into(),
            }],
            commands: if self.control {
                vec![Command {
                    id: id("resize"),
                    description: "Resize fixture".into(),
                    input_schema: json!({"type":"object"}),
                }]
            } else {
                vec![]
            },
            events: true,
            max_frame_bytes: MAX_FRAME_BYTES as u32,
            max_event_page_bytes: MAX_PAGE_BYTES as u32,
        })
    }

    fn state(&self, resource: Id, _: Value) -> HostFuture<'_, State> {
        Box::pin(async move {
            Ok(State {
                resource,
                schema_version: 1,
                revision: Some(Counter(1)),
                value: json!({"working":1}),
            })
        })
    }

    fn events(&self, _: Option<Counter>, _: u16, _: u16) -> HostFuture<'_, EventPage> {
        Box::pin(async {
            Ok(EventPage {
                events: vec![],
                start_cursor: Counter(0),
                next_cursor: Counter(0),
                live_cursor: Counter(0),
                retention_gap: false,
                has_more: false,
            })
        })
    }

    fn command(&self, command: Id, _: Value) -> HostFuture<'_, CommandResult> {
        Box::pin(async move {
            self.authorize(Access::Control)?;
            self.writes.fetch_add(1, Ordering::AcqRel);
            Ok(CommandResult {
                command,
                value: json!({"status":"queued","task_id":"task-1"}),
            })
        })
    }
}

fn request(name: &str, operation: Operation) -> Request {
    Request {
        version: VERSION,
        id: id(name),
        operation,
    }
}

fn discover(model: &mut BridgeModel, host: &Fixture) {
    ready(model.request_with(
        request("discover", Operation::Capabilities {}),
        |request| async move { Ok(zbridge::dispatch(host, request).await) },
    ))
    .unwrap();
}

fn read_state(model: &mut BridgeModel, host: &Fixture) {
    ready(model.request_with(
        request(
            "read",
            Operation::State {
                generation: id(host.generation),
                resource: id("status"),
                query: json!({}),
            },
        ),
        |request| async move { Ok(zbridge::dispatch(host, request).await) },
    ))
    .unwrap();
}

fn resize() -> Request {
    request(
        "resize-once",
        Operation::Command {
            generation: id("generation-1"),
            command: id("resize"),
            arguments: json!({"total":2}),
        },
    )
}

fn response(request: &Request, body: ResponseBody) -> Response {
    Response {
        version: VERSION,
        id: request.id.clone(),
        generation: Some(id("generation-1")),
        body,
    }
}

fn receipt() -> ResponseBody {
    ResponseBody::Command(CommandResult {
        command: id("resize"),
        value: json!({"status":"queued"}),
    })
}

fn assert_unknown(model: &BridgeModel) {
    let record = model.last_command().unwrap();
    assert_eq!(record.request_id, id("resize-once"));
    assert_eq!(record.generation, id("generation-1"));
    assert_eq!(record.command, id("resize"));
    assert!(record.is_uncertain());
    assert_eq!(model.last_error().unwrap().effect, Effect::MayHaveOccurred);
    assert!(model.last_error().unwrap().message.len() <= 1024);
    assert!(model.generation().is_none());
    assert!(model.state().is_none());
}

#[test]
fn typed_transport_uses_shared_host_and_keeps_native_receipt() {
    let host = Fixture::new(true);
    let mut model = BridgeModel::default();
    discover(&mut model, &host);
    read_state(&mut model, &host);
    assert_eq!(model.state().unwrap().value["working"], 1);
    let result = ready(model.request_with(resize(), |request| async {
        Ok(zbridge::dispatch(&host, request).await)
    }))
    .unwrap();
    let ResponseBody::Command(receipt) = result else {
        panic!("expected native receipt")
    };
    assert_eq!(receipt.value, json!({"status":"queued","task_id":"task-1"}));
    assert_eq!(host.writes.load(Ordering::Acquire), 1);
    assert!(model.state().is_none());
    assert!(!model.last_command().unwrap().is_uncertain());
    let CommandOutcome::Receipt(saved) = &model.last_command().unwrap().outcome else {
        panic!("expected retained native receipt")
    };
    assert_eq!(saved.value, receipt.value);

    model.disconnect();
    let mut next = Fixture::new(true);
    next.generation = "generation-2";
    discover(&mut model, &next);
    assert_eq!(model.generation(), Some(&id("generation-2")));
    assert_eq!(model.last_command().unwrap().generation, id("generation-1"));
    assert_eq!(next.writes.load(Ordering::Acquire), 0);
    assert_eq!(host.writes.load(Ordering::Acquire), 1);
}

#[test]
fn observer_and_local_validation_never_invoke_transport() {
    let host = Fixture::new(false);
    let mut model = BridgeModel::default();
    discover(&mut model, &host);
    let calls = Cell::new(0);
    let error = ready(model.request_with(resize(), |_| {
        calls.set(calls.get() + 1);
        async {
            Err(BridgeError::new(
                ErrorCode::Internal,
                "unexpected transport",
            ))
        }
    }))
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::Denied);
    assert_eq!(error.effect, Effect::None);
    let invalid = request(
        "invalid",
        Operation::State {
            generation: id("generation-1"),
            resource: id("status"),
            query: json!([]),
        },
    );
    assert_eq!(
        ready(model.request_with(invalid, |_| {
            calls.set(calls.get() + 1);
            async {
                Err(BridgeError::new(
                    ErrorCode::Internal,
                    "unexpected transport",
                ))
            }
        }))
        .unwrap_err()
        .effect,
        Effect::None
    );
    assert_eq!(calls.get(), 0);
    assert_eq!(host.writes.load(Ordering::Acquire), 0);
    assert!(model.last_command().is_none());
}

#[test]
fn failed_transport_and_mismatched_replies_remain_unknown_without_replay() {
    for failure in 0..4 {
        let host = Fixture::new(true);
        let mut model = BridgeModel::default();
        discover(&mut model, &host);
        read_state(&mut model, &host);
        let calls = Cell::new(0);
        let error = ready(model.request_with(resize(), |request| {
            calls.set(calls.get() + 1);
            async move {
                if failure == 0 {
                    return Err(BridgeError {
                        code: ErrorCode::Unavailable,
                        message: "失".repeat(2000),
                        effect: Effect::None,
                    });
                }
                let mut reply = response(&request, receipt());
                match failure {
                    1 => reply.id = id("other-request"),
                    2 => reply.generation = Some(id("generation-2")),
                    3 => {
                        reply.body = ResponseBody::State(State {
                            resource: id("status"),
                            schema_version: 1,
                            revision: None,
                            value: json!({}),
                        })
                    }
                    _ => unreachable!(),
                }
                Ok(reply)
            }
        }))
        .unwrap_err();
        assert_eq!(error.effect, Effect::MayHaveOccurred);
        assert_eq!(calls.get(), 1);
        assert_unknown(&model);
        discover(&mut model, &host);
        assert!(model.last_command().unwrap().is_uncertain());
        assert_eq!(calls.get(), 1);
        assert_eq!(host.writes.load(Ordering::Acquire), 0);
    }
}

#[test]
fn dropping_an_in_flight_command_preserves_its_unknown_effect() {
    let host = Fixture::new(true);
    let mut model = BridgeModel::default();
    discover(&mut model, &host);
    let calls = Cell::new(0);
    // An unpolled future has neither invoked transport nor attempted a command.
    drop(model.request_with(resize(), |_| {
        calls.set(calls.get() + 1);
        pending::<Result<Response, BridgeError>>()
    }));
    assert_eq!(calls.get(), 0);
    assert!(model.last_command().is_none());
    assert_eq!(model.generation(), Some(&id("generation-1")));
    let mut future = Box::pin(model.request_with(resize(), |_| {
        calls.set(calls.get() + 1);
        pending::<Result<Response, BridgeError>>()
    }));
    let waker = Waker::noop();
    assert!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(waker))
            .is_pending()
    );
    drop(future);
    assert_eq!(calls.get(), 1);
    assert_unknown(&model);
    discover(&mut model, &host);
    assert_eq!(calls.get(), 1);
    assert!(model.last_command().unwrap().is_uncertain());
}

#[test]
fn native_errors_keep_the_original_request_and_never_bind_error_generation() {
    for effect in [Effect::None, Effect::MayHaveOccurred] {
        let host = Fixture::new(true);
        let mut model = BridgeModel::default();
        discover(&mut model, &host);
        let error = ready(model.request_with(resize(), |request| async move {
            let mut reply = response(
                &request,
                ResponseBody::Error(BridgeError {
                    code: ErrorCode::Unavailable,
                    message: "native result".into(),
                    effect,
                }),
            );
            reply.generation = Some(id("generation-2"));
            Ok(reply)
        }))
        .unwrap_err();
        assert_eq!(error.effect, effect);
        assert!(model.generation().is_none());
        let record = model.last_command().unwrap();
        assert_eq!(record.generation, id("generation-1"));
        assert_eq!(record.is_uncertain(), effect == Effect::MayHaveOccurred);
    }
}

#[test]
fn retention_gaps_and_capability_refreshes_replace_live_projections() {
    let host = Fixture::new(true);
    let mut model = BridgeModel::default();
    discover(&mut model, &host);
    read_state(&mut model, &host);
    let events = request(
        "events",
        Operation::Events {
            generation: id("generation-1"),
            cursor: Some(Counter(2)),
            limit: 4,
            wait_ms: 0,
        },
    );
    ready(model.request_with(events, |request| async move {
        Ok(response(
            &request,
            ResponseBody::Events(EventPage {
                events: vec![Event {
                    sequence: Counter(5),
                    topic: id("update"),
                    value: json!({"fresh":true}),
                }],
                start_cursor: Counter(5),
                next_cursor: Counter(6),
                live_cursor: Counter(6),
                retention_gap: true,
                has_more: false,
            }),
        ))
    }))
    .unwrap();
    assert!(model.state().is_none());
    assert!(model.events().unwrap().retention_gap);
    assert_eq!(model.events().unwrap().events.len(), 1);
    read_state(&mut model, &host);
    assert!(model.state().is_some());
    discover(&mut model, &host);
    assert!(model.state().is_none());
    assert!(model.events().is_none());
}

#[test]
fn json_transport_keeps_large_cursors_as_lossless_decimal_strings() {
    let host = Fixture::new(false);
    let mut model = BridgeModel::default();
    discover(&mut model, &host);
    let cursor = Counter((1u64 << 53) + 1);
    let calls = Cell::new(0);
    let events = request(
        "large-cursor",
        Operation::Events {
            generation: id("generation-1"),
            cursor: Some(cursor),
            limit: 4,
            wait_ms: 0,
        },
    );
    ready(model.request_json_with(events, |bytes| {
        calls.set(calls.get() + 1);
        async move {
            let value: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(value["operation"]["cursor"], cursor.0.to_string());
            let request = zbridge::decode_request(&bytes)?;
            zbridge::encode_response(&response(
                &request,
                ResponseBody::Events(EventPage {
                    events: vec![],
                    start_cursor: Counter(0),
                    next_cursor: cursor,
                    live_cursor: cursor,
                    retention_gap: false,
                    has_more: false,
                }),
            ))
        }
    }))
    .unwrap();
    assert_eq!(calls.get(), 1);
    assert_eq!(model.events().unwrap().next_cursor, cursor);
}

#[test]
fn advertised_limits_and_schema_changes_reject_stale_cache_updates() {
    for invalid in 0..3 {
        let host = Fixture::new(false);
        let mut model = BridgeModel::default();
        ready(model.request_with(
            request("limits", Operation::Capabilities {}),
            |request| async move {
                let mut capabilities = host.capabilities()?;
                capabilities.max_frame_bytes = 1024;
                capabilities.max_event_page_bytes = 256;
                Ok(response(&request, ResponseBody::Capabilities(capabilities)))
            },
        ))
        .unwrap();
        let operation = if invalid == 2 {
            Operation::Events {
                generation: id("generation-1"),
                cursor: Some(Counter(0)),
                limit: 4,
                wait_ms: 0,
            }
        } else {
            Operation::State {
                generation: id("generation-1"),
                resource: id("status"),
                query: json!({}),
            }
        };
        let error = ready(model.request_with(
            request("invalid-cache", operation),
            |request| async move {
                let body = if invalid == 2 {
                    ResponseBody::Events(EventPage {
                        events: vec![Event {
                            sequence: Counter(0),
                            topic: id("large"),
                            value: json!({"data":"x".repeat(300)}),
                        }],
                        start_cursor: Counter(0),
                        next_cursor: Counter(1),
                        live_cursor: Counter(1),
                        retention_gap: false,
                        has_more: false,
                    })
                } else {
                    ResponseBody::State(State {
                        resource: id("status"),
                        schema_version: if invalid == 0 { 2 } else { 1 },
                        revision: None,
                        value: if invalid == 1 {
                            json!({"data":"x".repeat(2000)})
                        } else {
                            json!({})
                        },
                    })
                };
                Ok(response(&request, body))
            },
        ))
        .unwrap_err();
        assert_eq!(
            error.code,
            if invalid == 0 {
                ErrorCode::Conflict
            } else {
                ErrorCode::TooLarge
            }
        );
        assert_eq!(error.effect, Effect::None);
        assert!(model.state().is_none());
        assert!(model.events().is_none());
        assert!(model.generation().is_none());
    }
}
