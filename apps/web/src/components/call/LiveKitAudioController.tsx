"use client";

import React, { useEffect } from "react";
import { RoomAudioRenderer, useLocalParticipant, useRoomContext } from "@livekit/components-react";

interface LiveKitAudioControllerProps {
  isMicMuted: boolean;
  isSpeakerMuted: boolean;
}

export function LiveKitAudioController({
  isMicMuted,
  isSpeakerMuted,
}: LiveKitAudioControllerProps): React.ReactElement {
  const { localParticipant } = useLocalParticipant();
  const room = useRoomContext();

  // Unlock WebAudio context on mount — browsers block autoplay until a user
  // gesture has occurred. Calling resume() here ensures the AI voice track is
  // audible as soon as LiveKit delivers the first audio frames.
  useEffect(() => {
    const unlockAudio = async () => {
      try {
        const AudioCtx =
          window.AudioContext ||
          (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext;
        if (AudioCtx) {
          const ctx = new AudioCtx();
          if (ctx.state === "suspended") {
            await ctx.resume();
          }
        }
      } catch {
        // Non-blocking — ignore if AudioContext is unavailable
      }
    };

    void unlockAudio();
  }, []);

  // Control physical microphone track in WebRTC
  useEffect(() => {
    if (localParticipant) {
      localParticipant.setMicrophoneEnabled(!isMicMuted).catch((err: unknown) => {
        console.warn("Failed to set microphone state:", err);
      });
    }
  }, [localParticipant, isMicMuted]);

  // Control remote audio tracks in WebRTC
  useEffect(() => {
    if (room) {
      room.remoteParticipants.forEach((p) => {
        p.audioTrackPublications.forEach((pub) => {
          if (pub.track?.mediaStreamTrack) {
            pub.track.mediaStreamTrack.enabled = !isSpeakerMuted;
          }
        });
      });
    }
  }, [room, isSpeakerMuted]);

  return <RoomAudioRenderer muted={isSpeakerMuted} volume={isSpeakerMuted ? 0 : 1} />;
}
