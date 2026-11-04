"use client";

import React from "react";

interface RecordingAudioProps {
  callId: string;
  src: string;
  className?: string;
}

/** Keep an active recording stable even if its parent receives refreshed metadata. */
export function RecordingAudio({ callId, src, className = "" }: RecordingAudioProps): React.ReactElement {
  const audioRef = React.useRef<HTMLAudioElement>(null);
  const [stableSrc, setStableSrc] = React.useState(src);

  React.useEffect(() => {
    // A new presigned URL must not reset an active browser audio stream.
    if (audioRef.current?.paused) setStableSrc(src);
  }, [src]);

  return (
    <audio
      ref={audioRef}
      key={callId}
      controls
      preload="none"
      src={stableSrc}
      className={`h-9 w-full min-w-0 ${className}`}
      onClick={(event) => event.stopPropagation()}
    />
  );
}

export function isPlayableUrl(url?: string | null): url is string {
  return typeof url === "string" && url.startsWith("https://");
}
