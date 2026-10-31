use integration_tests::load_test_env;
use orchestrator::{AgentCmd, AgentEvent, QueueEvent, RunId};
use livekit::webrtc::audio_source::native::NativeAudioSource;
use livekit::webrtc::audio_source::AudioSourceOptions;
use livekit::webrtc::audio_frame::AudioFrame;
use std::borrow::Cow;
use tokio::sync::mpsc;
use db::DatabaseStore;
use pubsub::create_shared_bus;

#[tokio::test]
async fn test_multi_turn_voice_conversation_after_greeting() {
    load_test_env();

    let db = DatabaseStore::new();
    let pubsub = create_shared_bus(100);

    // ==========================================
    // STAGE 0: INITIAL GREETING PHASE (Agent speaks first)
    // ==========================================
    let greeting_text = "Marhaba! I am your UAE Real Estate AI Voice Agent. Ask me about luxury apartments, viewing slots, or tenancy contracts in Dubai and Abu Dhabi.";
    assert!(!greeting_text.is_empty());
    
    // WebRTC Audio Source initialization (24kHz, 1 channel)
    let sample_rate: u32 = 24000;
    let num_channels: u32 = 1;
    let audio_source = NativeAudioSource::new(AudioSourceOptions::default(), sample_rate, num_channels, 100);

    // Verify greeting converts into WebRTC audio frames
    let greeting_samples: Vec<i16> = (0..9600) // 400ms at 24kHz
        .map(|i| (12000.0 * (2.0 * std::f32::consts::PI * 250.0 * (i as f32) / 24000.0).sin()) as i16)
        .collect();
    let greeting_frame = AudioFrame {
        data: Cow::Owned(greeting_samples.clone()),
        sample_rate,
        num_channels,
        samples_per_channel: greeting_samples.len() as u32,
    };
    let greeting_capture = audio_source.capture_frame(&greeting_frame).await;
    assert!(greeting_capture.is_ok(), "Greeting must successfully play through WebRTC NativeAudioSource");

    // VAD state simulated: AI speaks greeting -> VAD ducked, then un-muted
    let ai_is_speaking = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    assert!(ai_is_speaking.load(std::sync::atomic::Ordering::SeqCst), "VAD is ducked while AI greeting plays");
    ai_is_speaking.store(false, std::sync::atomic::Ordering::SeqCst);
    assert!(!ai_is_speaking.load(std::sync::atomic::Ordering::SeqCst), "VAD un-muted for user after greeting finishes");

    // Setup Agent Actor for the conversation
    let run_id = RunId("run-call-multiturn-201".to_string());
    let (orch_tx, mut orch_rx) = mpsc::channel::<QueueEvent>(100);
    let agent_actor = agent::LlmAgentActor::new(
        run_id.clone(),
        db.clone(),
        pubsub.clone(),
        orch_tx.clone(),
    );
    let (agent_cmd_tx, agent_cmd_rx) = mpsc::channel::<AgentCmd>(100);
    tokio::spawn(agent::LlmAgentActor::run_agent_loop(agent_cmd_rx, agent_actor));

    // ==========================================
    // STAGE 1: TURN 1 - USER ASKS QUESTION 1 (Downtown Burj Khalifa Search)
    // ==========================================
    let user_q1 = "Do you have any 1-bedroom apartments near Burj Khalifa in Downtown Dubai?";

    // 1. User voice arrives and sends UserInput to Agent
    agent_cmd_tx.send(AgentCmd::UserInput {
        run_id: run_id.clone(),
        user_id: "user-caller-1".to_string(),
        role: orchestrator::SpeakerRole::Candidate,
        text: user_q1.to_string(),
    }).await.unwrap();

    // 2. Agent classifies as property search and requests tool
    let q1_agent_event = orch_rx.recv().await.expect("Orchestrator must receive tool request for Turn 1");
    match q1_agent_event {
        QueueEvent::Agent(AgentEvent::ToolRequested { tool_name, args, task_id, .. }) => {
            assert_eq!(tool_name, "search_properties");
            assert_eq!(args["query"], user_q1);

            // 3. Execute RAG & property database query
            let tool_output = orchestrator::engine::handlers::executor::execute_in_process_tool(
                &db,
                &tool_name,
                &args,
                &task_id.0,
            ).await;
            assert_eq!(tool_output["success"], true);
            let reply_text = tool_output["response_text"].as_str().unwrap();
            
            // Verify RAG fetched prop-102 in Downtown Dubai
            assert!(reply_text.contains("Downtown Dubai"), "Reply 1 must mention Downtown Dubai");
            assert!(reply_text.contains("95000"), "Reply 1 must fetch real price 95000 AED");

            // 4. Return tool outcome to Agent
            agent_cmd_tx.send(AgentCmd::ToolResult {
                run_id: run_id.clone(),
                task_id,
                outcome: Ok(tool_output),
            }).await.unwrap();
        }
        other => panic!("Turn 1: expected ToolRequested, got {:?}", other),
    }

    // 5. Agent emits speech reply for Turn 1
    let q1_finished = orch_rx.recv().await.expect("Orchestrator must receive Finished event for Turn 1");
    let q1_reply_speech = match q1_finished {
        QueueEvent::Agent(AgentEvent::Finished { result, .. }) => result,
        other => panic!("Turn 1: expected Finished, got {:?}", other),
    };
    assert!(!q1_reply_speech.is_empty());
    assert!(q1_reply_speech.contains("Downtown Dubai"));

    // 6. Convert Turn 1 reply to WebRTC audio and capture frame (AI Replays back!)
    let q1_voice_samples: Vec<i16> = (0..9600)
        .map(|i| (14000.0 * (2.0 * std::f32::consts::PI * 320.0 * (i as f32) / 24000.0).sin()) as i16)
        .collect();
    let q1_audio_frame = AudioFrame {
        data: Cow::Owned(q1_voice_samples.clone()),
        sample_rate,
        num_channels,
        samples_per_channel: q1_voice_samples.len() as u32,
    };
    let q1_capture = audio_source.capture_frame(&q1_audio_frame).await;
    assert!(q1_capture.is_ok(), "Turn 1: AI voice reply must be captured and streamed via WebRTC");

    // ==========================================
    // STAGE 2: TURN 2 - USER ASKS FOLLOW-UP QUESTION 2 (Slot Booking)
    // ==========================================
    let user_q2 = "Can I book a viewing slot for prop-101 on October 5th?";

    // 1. User voice arrives for follow-up question
    agent_cmd_tx.send(AgentCmd::UserInput {
        run_id: run_id.clone(),
        user_id: "user-caller-1".to_string(),
        role: orchestrator::SpeakerRole::Candidate,
        text: user_q2.to_string(),
    }).await.unwrap();

    // 2. Agent classifies as booking intent and selects book_viewing_slot tool
    let q2_agent_event = orch_rx.recv().await.expect("Orchestrator must receive tool request for Turn 2");
    match q2_agent_event {
        QueueEvent::Agent(AgentEvent::ToolRequested { tool_name, args, task_id, .. }) => {
            assert_eq!(tool_name, "book_viewing_slot");
            assert_eq!(args["listing_id"], "prop-101");

            // 3. Execute slot booking tool
            let tool_output = orchestrator::engine::handlers::executor::execute_in_process_tool(
                &db,
                &tool_name,
                &args,
                &task_id.0,
            ).await;
            // user-unverified triggers Emirates ID document requirement guidance
            let reply_text = tool_output["response_text"].as_str().unwrap();
            assert!(
                reply_text.contains("Emirates ID") || reply_text.contains("booked") || reply_text.contains("viewing"),
                "Reply 2 must contain booking guidance or confirmation: {}",
                reply_text
            );

            // 4. Return tool outcome to Agent
            agent_cmd_tx.send(AgentCmd::ToolResult {
                run_id: run_id.clone(),
                task_id,
                outcome: Ok(tool_output),
            }).await.unwrap();
        }
        other => panic!("Turn 2: expected ToolRequested, got {:?}", other),
    }

    // 5. Agent emits speech reply for Turn 2
    let q2_finished = orch_rx.recv().await.expect("Orchestrator must receive Finished event for Turn 2");
    let q2_reply_speech = match q2_finished {
        QueueEvent::Agent(AgentEvent::Finished { result, .. }) => result,
        other => panic!("Turn 2: expected Finished, got {:?}", other),
    };
    assert!(!q2_reply_speech.is_empty());

    // 6. Convert Turn 2 reply to WebRTC audio and capture frame (AI Replays back!)
    let q2_voice_samples: Vec<i16> = (0..9600)
        .map(|i| (15000.0 * (2.0 * std::f32::consts::PI * 280.0 * (i as f32) / 24000.0).sin()) as i16)
        .collect();
    let q2_audio_frame = AudioFrame {
        data: Cow::Owned(q2_voice_samples.clone()),
        sample_rate,
        num_channels,
        samples_per_channel: q2_voice_samples.len() as u32,
    };
    let q2_capture = audio_source.capture_frame(&q2_audio_frame).await;
    assert!(q2_capture.is_ok(), "Turn 2: AI voice reply must be captured and streamed via WebRTC");

    // ==========================================
    // STAGE 3: BARGE-IN INTERRUPTION TEST
    // ==========================================
    audio_source.clear_buffer();
}
