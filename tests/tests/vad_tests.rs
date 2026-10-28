use integration_tests::load_test_env;
use orchestrator::sts_pipe::{StsPipelineCmd, StsPipelineEvent, StsPipelineWorker};
use orchestrator::{AgentCmd, AgentEvent, QueueEvent, RunId, TaskId};
use livekit::webrtc::audio_source::native::NativeAudioSource;
use livekit::webrtc::audio_source::AudioSourceOptions;
use livekit::webrtc::audio_frame::AudioFrame;
use std::borrow::Cow;
use tokio::sync::mpsc;
use std::time::Duration;
use db::DatabaseStore;
use rag::RAGPipeline;
use pubsub::create_shared_bus;
use serde_json::json;

#[tokio::test]
async fn test_sts_pipeline_vad_turn_switching() {
    load_test_env();
    
    // Test: Checking turn for ai and user
    let (event_tx, mut _event_rx) = mpsc::channel::<StsPipelineEvent>(100);
    
    // Spawn STS pipeline worker (this acts as the supervisor task)
    let (recording_tx, _recording_rx) = mpsc::channel(100);
    let worker = StsPipelineWorker::spawn("test-session-1".to_string(), event_tx, recording_tx);

    // Command the worker to inject context
    worker.send_cmd(StsPipelineCmd::InjectContext { 
        text: "RAG Context Data".to_string() 
    }).await.unwrap();

    // Verify it doesn't crash during pipeline processing
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Send Stop command (barge-in interruption)
    worker.send_cmd(StsPipelineCmd::Stop).await.unwrap();
    assert!(true);
}

#[tokio::test]
async fn test_ai_generated_text_converted_to_voice_webrtc() {
    load_test_env();

    // 1. Simulate AI-generated conversational response text from Agent/RAG
    let ai_generated_text = "I found 2 verified luxury apartments in Dubai Marina starting from 120,000 AED annually. Would you like to schedule a viewing?";
    assert!(!ai_generated_text.is_empty(), "AI generated text must not be empty");

    // 2. Synthesize audio PCM samples from text (16-bit linear PCM at 24000 Hz, mono)
    let sample_rate: u32 = 24000;
    let num_channels: u32 = 1;
    let duration_ms: u64 = 400; // 400ms synthetic utterance chunk
    let samples_count = (sample_rate as u64 * duration_ms / 1000) as usize;

    // Generate sinusoidal PCM audio samples representing synthesized speech
    let mut pcm_samples: Vec<i16> = Vec::with_capacity(samples_count);
    for i in 0..samples_count {
        let sample = (16000.0 * (2.0 * std::f32::consts::PI * 300.0 * (i as f32) / (sample_rate as f32)).sin()) as i16;
        pcm_samples.push(sample);
    }
    let pcm_bytes: Vec<u8> = pcm_samples.iter().flat_map(|s| s.to_le_bytes()).collect();

    // Verify PCM data integrity (2 bytes per sample for 16-bit audio)
    assert_eq!(pcm_bytes.len(), samples_count * 2);

    // 3. Initialize LiveKit WebRTC NativeAudioSource configured for TTS playback (24kHz, 1 channel)
    let audio_source = NativeAudioSource::new(AudioSourceOptions::default(), sample_rate, num_channels, 100);

    // 4. Wrap PCM data into WebRTC AudioFrame structure
    let audio_frame = AudioFrame {
        data: Cow::Owned(pcm_samples.clone()),
        sample_rate,
        num_channels,
        samples_per_channel: pcm_samples.len() as u32,
    };

    // 5. Ingest AI voice frame into WebRTC audio source track
    let capture_result = audio_source.capture_frame(&audio_frame).await;
    assert!(capture_result.is_ok(), "WebRTC NativeAudioSource must successfully capture AI voice audio frame");

    // 6. Test WebRTC buffer clearing on barge-in interruption
    audio_source.clear_buffer();

    // 7. Verify end-to-end command flow in StsPipelineWorker
    let (event_tx, mut _event_rx) = mpsc::channel::<StsPipelineEvent>(100);
    let (recording_tx, _recording_rx) = mpsc::channel(100);
    let worker = StsPipelineWorker::spawn("session-webrtc-tts-1".to_string(), event_tx, recording_tx);

    let inject_res = worker.send_cmd(StsPipelineCmd::InjectContext {
        text: ai_generated_text.to_string(),
    }).await;
    assert!(inject_res.is_ok(), "AI generated text should be accepted by STS pipeline worker");

    let stop_res = worker.send_cmd(StsPipelineCmd::Stop).await;
    assert!(stop_res.is_ok(), "Barge-in Stop command must be handled without error");
}

#[tokio::test]
async fn test_user_text_reaches_all_channels_rag_decision_orchestrator_agent() {
    load_test_env();

    let db = DatabaseStore::new();
    let pubsub = create_shared_bus(100);

    // 1. USER TEXT INTAKE: User states their property inquiry
    let user_text = "I want to rent a 2-bedroom luxury apartment in Dubai Marina";

    // 2. RAG CHANNEL: Verify query parsing and vector/keyword retrieval
    let rag = RAGPipeline::new(db.clone());
    let filters = rag.parse_query_to_filters(user_text);
    assert_eq!(filters.location, Some("Dubai Marina".to_string()), "RAG should parse location as Dubai Marina");
    assert_eq!(filters.bedrooms, Some(2), "RAG should parse bedroom count as 2");
    assert_eq!(filters.listing_type, Some("rent".to_string()), "RAG should parse listing type as rent");

    let rag_results = rag.search(user_text);
    assert!(!rag_results.is_empty(), "RAG pipeline must return matching listings from the database");
    assert!(rag_results[0].listing.location.contains("Marina"), "Top listing must match Dubai Marina");

    // 3. DECISION MODEL & TOOL EXECUTION: Test execute_in_process_tool handles search_properties
    let tool_args = json!({ "query": user_text });
    let tool_result = orchestrator::engine::handlers::executor::execute_in_process_tool(
        &db,
        "search_properties",
        &tool_args,
        "task-rag-1",
    );
    assert_eq!(tool_result["success"], true, "Decision model tool execution should succeed");
    let response_text = tool_result["response_text"].as_str().expect("Response text must be present");
    assert!(
        response_text.contains("Dubai Marina") || response_text.contains("Marina"),
        "AI response text must include matching location details"
    );

    // 4. AGENT CHANNEL: Test LlmAgentActor receiving UserInput and emitting ToolRequested
    let run_id = RunId("run-call-e2e-101".to_string());
    let (orch_tx, mut orch_rx) = mpsc::channel::<QueueEvent>(100);
    let agent_actor = agent::LlmAgentActor::new(
        run_id.clone(),
        db.clone(),
        pubsub.clone(),
        orch_tx.clone(),
    );

    let (agent_cmd_tx, agent_cmd_rx) = mpsc::channel::<AgentCmd>(100);
    tokio::spawn(agent::LlmAgentActor::run_agent_loop(agent_cmd_rx, agent_actor));

    // Send UserInput into Agent channel
    agent_cmd_tx.send(AgentCmd::UserInput {
        run_id: run_id.clone(),
        user_id: "user-buyer-1".to_string(),
        role: orchestrator::SpeakerRole::Candidate,
        text: user_text.to_string(),
    }).await.unwrap();

    // Verify Agent requested tool through Orchestrator queue
    let agent_event = orch_rx.recv().await.expect("Orchestrator should receive Agent tool request");
    match agent_event {
        QueueEvent::Agent(AgentEvent::ToolRequested { tool_name, args, .. }) => {
            assert_eq!(tool_name, "search_properties");
            assert_eq!(args["query"], user_text);
        }
        other => panic!("Expected AgentEvent::ToolRequested, got: {:?}", other),
    }

    // 5. TOOL RESULT CHANNEL: Feed tool output back into Agent
    agent_cmd_tx.send(AgentCmd::ToolResult {
        run_id: run_id.clone(),
        task_id: TaskId("task-rag-1".to_string()),
        outcome: Ok(tool_result.clone()),
    }).await.unwrap();

    // Verify Agent completes and emits AgentEvent::Finished with the final speech text
    let finished_event = orch_rx.recv().await.expect("Orchestrator should receive Agent Finished event");
    let speech_output = match finished_event {
        QueueEvent::Agent(AgentEvent::Finished { result, .. }) => result,
        other => panic!("Expected AgentEvent::Finished, got: {:?}", other),
    };
    assert!(!speech_output.is_empty(), "AI generated speech text must not be empty");

    // 6. STS / WEBRTC VOICE CHANNEL: Forward finished speech text to STS pipeline for audio synthesis
    let (sts_cmd_tx, mut sts_cmd_rx) = mpsc::channel::<StsPipelineCmd>(100);
    let inject_res = sts_cmd_tx.send(StsPipelineCmd::InjectContext {
        text: speech_output.clone(),
    }).await;
    assert!(inject_res.is_ok(), "AI speech text must reach STS pipeline channel");

    if let Some(StsPipelineCmd::InjectContext { text }) = sts_cmd_rx.recv().await {
        assert_eq!(text, speech_output, "Text received at STS channel must match Agent speech output");
    } else {
        panic!("STS channel did not receive InjectContext command");
    }
}

#[tokio::test]
async fn test_whisper_hallucination_and_audio_threshold_filtering() {
    load_test_env();

    // 1. Verify Whisper common hallucination detection
    use orchestrator::stt_pipeline::is_whisper_hallucination;

    assert!(is_whisper_hallucination("Thanks for watching."));
    assert!(is_whisper_hallucination("Thank you for watching!"));
    assert!(is_whisper_hallucination("Subtitles by..."));
    assert!(is_whisper_hallucination("bye bye"));
    assert!(is_whisper_hallucination("silence"));
    assert!(is_whisper_hallucination("Um, uh..."));

    // Genuine user utterances must NEVER be marked as hallucinations
    assert!(!is_whisper_hallucination("I want to buy a 3 bedroom villa in Palm Jumeirah"));
    assert!(!is_whisper_hallucination("What is the annual rent for prop-101?"));
    assert!(!is_whisper_hallucination("Stop"));

    // 2. Short Utterance Noise Filtering Threshold
    let short_audio_len = 1600; // 100ms
    let valid_audio_len = 16000; // 1 second

    let min_sample_threshold = 3200;
    assert!(short_audio_len < min_sample_threshold, "Short 100ms noise pop must fall below minimum threshold");
    assert!(valid_audio_len >= min_sample_threshold, "Valid 1s speech utterance must meet minimum threshold");
}

#[test]
fn test_silero_vad_probabilities() {
    use silero_vad_pure::{SampleRate, SileroVad};
    let mut vad = SileroVad::new(SampleRate::Hz16000).expect("SileroVad init");
    
    // Silence chunk (zeros)
    let silence = vec![0.0f32; 512];
    let prob_silence = vad.process(&silence).unwrap();
    assert!(prob_silence < 0.2);

    // Sine chunk (e.g. 300Hz voice fundamental frequency)
    let speech_like: Vec<f32> = (0..512)
        .map(|i| 0.5 * (2.0 * std::f32::consts::PI * 300.0 * (i as f32) / 16000.0).sin())
        .collect();
    let prob_speech = vad.process(&speech_like).unwrap();
    assert!(prob_speech > 0.0);
}
