use super::*;

fn select_config(
    id: &str,
    name: &str,
    category: acp::SessionConfigOptionCategory,
    current: &str,
    options: &[(&str, &str)],
) -> acp::SessionConfigOption {
    acp::SessionConfigOption::select(
        id.to_string(),
        name.to_string(),
        current.to_string(),
        options
            .iter()
            .map(|(value, label)| {
                acp::SessionConfigSelectOption::new((*value).to_string(), (*label).to_string())
            })
            .collect::<Vec<_>>(),
    )
    .category(Some(category))
}

#[test]
fn disposition_separates_runtime_mode_from_effort_and_hides_other_configs() {
    let configs = vec![
        select_config(
            "model-choice",
            "Model",
            acp::SessionConfigOptionCategory::Model,
            "model-2",
            &[("model-1", "Model One"), ("model-2", "Model Two")],
        ),
        select_config(
            "permission-mode",
            "Permission mode",
            acp::SessionConfigOptionCategory::Mode,
            "agent",
            &[("ask", "Ask"), ("agent", "Agent")],
        ),
        select_config(
            "reasoning",
            "Reasoning",
            acp::SessionConfigOptionCategory::ThoughtLevel,
            "high",
            &[("low", "Low"), ("high", "High")],
        ),
        select_config(
            "style",
            "Style",
            acp::SessionConfigOptionCategory::Other("style".to_string()),
            "concise",
            &[("concise", "Concise")],
        ),
    ];
    let generic = Disposition::of(None, Some(&configs));
    assert_eq!(generic.current_model_id.as_deref(), Some("model-2"));
    assert_eq!(generic.model_config.as_deref(), Some("model-choice"));
    assert_eq!(generic.current_mode_id.as_deref(), Some("agent"));
    assert_eq!(generic.mode_config.as_deref(), Some("permission-mode"));
    assert_eq!(generic.configs.len(), 1);
    assert_eq!(generic.configs[0].id, "reasoning");
    assert_eq!(
        generic.configs[0].category,
        Some(crate::contract::SessionConfigCategory::Effort)
    );

    let dedicated_modes = SessionModeState::new(
        acp::SessionModeId::new("ask"),
        vec![
            SessionMode::new(acp::SessionModeId::new("ask"), "Ask"),
            SessionMode::new(acp::SessionModeId::new("agent"), "Agent"),
        ],
    );
    let dedicated = Disposition::of(Some(&dedicated_modes), Some(&configs));
    assert_eq!(dedicated.current_mode_id.as_deref(), Some("ask"));
    assert!(dedicated.mode_config.is_none());
    assert_eq!(dedicated.configs.len(), 1);
    assert_eq!(dedicated.configs[0].id, "reasoning");
}

#[tokio::test]
async fn config_notifications_update_model_and_effort_without_replacing_mode() {
    let live = Live::default();
    let dedicated_modes = SessionModeState::new(
        acp::SessionModeId::new("ask"),
        vec![
            SessionMode::new(acp::SessionModeId::new("ask"), "Ask"),
            SessionMode::new(acp::SessionModeId::new("agent"), "Agent"),
        ],
    );
    {
        let mut session = lock(&live.session);
        let disposition = Disposition::of(Some(&dedicated_modes), None);
        session.dedicated_modes = true;
        session.info.modes = disposition.modes;
        session.info.current_mode_id = disposition.current_mode_id;
    }
    let update = SessionUpdate::ConfigOptionUpdate(acp::ConfigOptionUpdate::new(vec![
        select_config(
            "model-choice",
            "Model",
            acp::SessionConfigOptionCategory::Model,
            "model-2",
            &[("model-1", "Model One"), ("model-2", "Model Two")],
        ),
        select_config(
            "permission-mode",
            "Permission mode",
            acp::SessionConfigOptionCategory::Mode,
            "agent",
            &[("ask", "Ask"), ("agent", "Agent")],
        ),
        select_config(
            "reasoning",
            "Reasoning",
            acp::SessionConfigOptionCategory::ThoughtLevel,
            "high",
            &[("low", "Low"), ("high", "High")],
        ),
    ]));
    translate(&live, update).await;

    {
        let session = lock(&live.session);
        assert_eq!(session.info.current_model_id, "model-2");
        assert_eq!(session.info.current_mode_id.as_deref(), Some("ask"));
        assert_eq!(
            session
                .info
                .modes
                .iter()
                .map(|mode| mode.id.as_str())
                .collect::<Vec<_>>(),
            ["ask", "agent"]
        );
        assert_eq!(session.info.configs.len(), 1);
        assert_eq!(session.info.configs[0].current_id.as_deref(), Some("high"));
    }

    // A later full-set notification can remove model and config-based
    // mode controls. Those old ids must disappear rather than remaining
    // callable through a stale picker.
    let removal =
        SessionUpdate::ConfigOptionUpdate(acp::ConfigOptionUpdate::new(vec![select_config(
            "reasoning",
            "Reasoning",
            acp::SessionConfigOptionCategory::ThoughtLevel,
            "low",
            &[("low", "Low"), ("high", "High")],
        )]));
    translate(&live, removal).await;
    let session = lock(&live.session);
    assert!(session.model_config.is_none());
    assert_eq!(session.info.models, [] as [crate::driver::ConfigChoice; 0]);
    assert_eq!(session.info.current_model_id, "");
    assert_eq!(session.info.current_mode_id.as_deref(), Some("ask"));
    assert_eq!(session.info.configs.len(), 1);
}

#[tokio::test]
async fn config_notifications_remove_config_mode_without_stale_picker() {
    let live = Live::default();
    let initial =
        SessionUpdate::ConfigOptionUpdate(acp::ConfigOptionUpdate::new(vec![select_config(
            "runtime-mode",
            "Runtime mode",
            acp::SessionConfigOptionCategory::Mode,
            "build",
            [("plan", "Plan"), ("build", "Build")].as_slice(),
        )]));
    translate(&live, initial).await;

    {
        let session = lock(&live.session);
        assert_eq!(session.mode_config.as_deref(), Some("runtime-mode"));
        assert_eq!(session.info.current_mode_id.as_deref(), Some("build"));
        assert_eq!(session.info.modes.len(), 2);
    }

    // The ACP notification is a full replacement. Removing the mode
    // option must make the old wire command and UI picker unavailable.
    let removal =
        SessionUpdate::ConfigOptionUpdate(acp::ConfigOptionUpdate::new(vec![select_config(
            "reasoning",
            "Reasoning",
            acp::SessionConfigOptionCategory::ThoughtLevel,
            "high",
            [("low", "Low"), ("high", "High")].as_slice(),
        )]));
    translate(&live, removal).await;

    let session = lock(&live.session);
    assert!(session.mode_config.is_none());
    assert_eq!(session.info.modes, [] as [crate::driver::ConfigChoice; 0]);
    assert!(session.info.current_mode_id.is_none());
    assert!(session.info.mode_label.is_none());
    assert_eq!(session.info.configs.len(), 1);
}

/// `session.set_model` and `session.set_mode` are two commands, and the
/// wire answers each socket's on its own task. Both read the connection
/// and the session; taking those two locks in opposite orders wedges both
/// threads for good — no timeout, no error, and every later call on this
/// driver queued behind them.
#[tokio::test]
async fn setting_a_config_and_a_mode_at_once_does_not_wedge_the_driver() {
    let held = room("lock-order-room");
    let driver = Arc::new(ChildAgent::new(
        scratch("lock-order"),
        "cursor".to_string(),
        String::new(),
        TeammateTools::new(&held, "ada"),
    ));
    let finished = Arc::new(AtomicUsize::new(0));
    const ROUNDS: usize = 50_000;

    for worker in 0..2 {
        let driver = driver.clone();
        let finished = finished.clone();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .build()
                .expect("a runtime for this worker");
            runtime.block_on(async {
                for _ in 0..ROUNDS {
                    let _ = match worker {
                        0 => driver.set_config("config", "value").await,
                        _ => driver.set_mode("mode").await,
                    };
                }
            });
            finished.fetch_add(1, Ordering::SeqCst);
        });
    }

    for _ in 0..600 {
        if finished.load(Ordering::SeqCst) == 2 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the driver stopped answering: two callers are holding each other's lock");
}

/// An agent whose sessions advertise its own reasoning option, `reasoning`,
/// at `high` until it is set, with the values `offered`; each setting it is
/// asked for is heard.
fn reasoning_agent(
    offered: &'static [(&'static str, &'static str)],
    set: Arc<Mutex<Vec<(String, String)>>>,
) -> impl std::future::Future<Output = ()> + Send + 'static {
    let transport = agent_pipes();
    let reasoning = move |current: &str| {
        select_config(
            "reasoning",
            "Reasoning",
            acp::SessionConfigOptionCategory::ThoughtLevel,
            current,
            offered,
        )
    };
    async move {
        let running = agent_client_protocol::Agent
            .builder()
            .name("reasoning")
            .on_receive_request(
                async move |request: InitializeRequest, responder, _cx| {
                    responder.respond(
                        InitializeResponse::new(request.protocol_version)
                            .agent_capabilities(AgentCapabilities::new()),
                    )
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |_request: NewSessionRequest,
                            responder: Responder<NewSessionResponse>,
                            _cx| {
                    responder.respond(
                        NewSessionResponse::new(SessionId::new("fresh-session"))
                            .config_options(vec![reasoning("high")]),
                    )
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                move |request: SetSessionConfigOptionRequest,
                      responder: Responder<acp::SetSessionConfigOptionResponse>,
                      _cx| {
                    let set = set.clone();
                    async move {
                        let value = match &request.value {
                            acp::SessionConfigOptionValue::ValueId { value } => value.0.to_string(),
                            _ => String::new(),
                        };
                        set.lock()
                            .unwrap()
                            .push((request.config_id.0.to_string(), value.clone()));
                        responder.respond(acp::SetSessionConfigOptionResponse::new(vec![
                            reasoning(&value),
                        ]))
                    }
                },
                agent_client_protocol::on_receive_request!(),
            )
            .connect_to(transport)
            .await;
        if let Err(error) = running {
            eprintln!("the reasoning agent ended: {error}");
        }
    }
}

/// The effort the person picked is put back on the harness's own option when
/// the next session starts, while the harness still offers that value; one it
/// no longer offers, or none stored, leaves the harness's default.
#[tokio::test(flavor = "multi_thread")]
async fn a_stored_effort_is_asked_for_again_only_while_the_harness_offers_it() {
    for (stored, offered, asked, runs_at) in [
        (
            Some("low"),
            &[("low", "Low"), ("high", "High")][..],
            vec![("reasoning".to_string(), "low".to_string())],
            "low",
        ),
        (
            Some("low"),
            &[("medium", "Medium"), ("high", "High")][..],
            vec![],
            "high",
        ),
        (
            Some("high"),
            &[("low", "Low"), ("high", "High")][..],
            vec![],
            "high",
        ),
        (
            None,
            &[("low", "Low"), ("high", "High")][..],
            vec![],
            "high",
        ),
    ] {
        let held = room("effort-room");
        let set = Arc::new(Mutex::new(Vec::new()));
        tokio::spawn(reasoning_agent(offered, set.clone()));
        let driver = ChildAgent::new(
            scratch("effort"),
            "cursor".to_string(),
            String::new(),
            TeammateTools::new(&held, "ada"),
        );
        let mut ada = persona(&scratch_cwd(), Vec::new());
        ada.effort_id = stored.map(str::to_string);
        let info = driver.handshake(&ada, client_transport()).await.unwrap();
        assert_eq!(*set.lock().unwrap(), asked, "{stored:?} {offered:?}");
        assert_eq!(info.configs.len(), 1);
        assert_eq!(
            info.configs[0].current_id.as_deref(),
            Some(runs_at),
            "{stored:?} {offered:?}"
        );
    }
}
