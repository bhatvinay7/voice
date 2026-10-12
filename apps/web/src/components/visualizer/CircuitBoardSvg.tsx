"use client";

import React from "react";

export function CircuitBoardSvg(): React.ReactElement {
  return (
    <svg
      className="absolute inset-0 w-full h-full pointer-events-none z-0"
      viewBox="0 0 450 450"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
    >
      {/* Corner Background Constellation Lines */}
      <g opacity="0.35">
        <line x1="20" y1="50" x2="70" y2="20" stroke="#38bdf8" strokeWidth="0.8" />
        <line x1="70" y1="20" x2="110" y2="45" stroke="#38bdf8" strokeWidth="0.8" />
        <circle cx="20" cy="50" r="2" fill="#38bdf8" />
        <circle cx="70" cy="20" r="2.5" fill="#ffffff" />
        <circle cx="110" cy="45" r="2" fill="#38bdf8" />

        <line x1="380" y1="30" x2="430" y2="60" stroke="#38bdf8" strokeWidth="0.8" />
        <line x1="340" y1="50" x2="380" y2="30" stroke="#38bdf8" strokeWidth="0.8" />
        <circle cx="380" cy="30" r="2" fill="#38bdf8" />
        <circle cx="430" cy="60" r="2.5" fill="#ffffff" />

        <line x1="25" y1="400" x2="75" y2="430" stroke="#38bdf8" strokeWidth="0.8" />
        <line x1="75" y1="430" x2="110" y2="405" stroke="#38bdf8" strokeWidth="0.8" />
        <circle cx="25" cy="400" r="2" fill="#38bdf8" />
        <circle cx="75" cy="430" r="2.5" fill="#ffffff" />

        <line x1="375" y1="420" x2="425" y2="390" stroke="#38bdf8" strokeWidth="0.8" />
        <circle cx="425" cy="390" r="2.5" fill="#ffffff" />
      </g>

      {/* TOP Circuit Traces */}
      <path
        d="M 225 75 L 225 15"
        stroke="rgba(56, 189, 248, 0.7)"
        strokeWidth="1.6"
        className="circuit-pulse-line"
      />
      <circle cx="225" cy="14" r="3.2" fill="#38bdf8" className="drop-shadow-[0_0_8px_#38bdf8]" />

      <path
        d="M 200 82 L 200 48 L 175 24 L 175 12"
        stroke="rgba(56, 189, 248, 0.55)"
        strokeWidth="1.5"
      />
      <circle cx="175" cy="12" r="2.5" fill="#ffffff" className="drop-shadow-[0_0_5px_#ffffff]" />

      <path
        d="M 250 82 L 250 48 L 275 24 L 275 12"
        stroke="rgba(56, 189, 248, 0.55)"
        strokeWidth="1.5"
      />
      <circle cx="275" cy="12" r="2.5" fill="#ffffff" className="drop-shadow-[0_0_5px_#ffffff]" />

      <path
        d="M 175 98 L 150 72 L 125 72 L 105 52 L 105 30"
        stroke="rgba(56, 189, 248, 0.45)"
        strokeWidth="1.2"
      />
      <circle cx="105" cy="28" r="2" fill="#38bdf8" />

      <path
        d="M 275 98 L 300 72 L 325 72 L 345 52 L 345 30"
        stroke="rgba(56, 189, 248, 0.45)"
        strokeWidth="1.2"
      />
      <circle cx="345" cy="28" r="2" fill="#38bdf8" />

      {/* BOTTOM Circuit Traces */}
      <path
        d="M 225 375 L 225 435"
        stroke="rgba(56, 189, 248, 0.7)"
        strokeWidth="1.6"
        className="circuit-pulse-line"
      />
      <circle cx="225" cy="436" r="3.2" fill="#38bdf8" className="drop-shadow-[0_0_8px_#38bdf8]" />

      <path
        d="M 200 368 L 200 402 L 175 426 L 175 438"
        stroke="rgba(56, 189, 248, 0.55)"
        strokeWidth="1.5"
      />
      <circle cx="175" cy="438" r="2.5" fill="#ffffff" className="drop-shadow-[0_0_5px_#ffffff]" />

      <path
        d="M 250 368 L 250 402 L 275 426 L 275 438"
        stroke="rgba(56, 189, 248, 0.55)"
        strokeWidth="1.5"
      />
      <circle cx="275" cy="438" r="2.5" fill="#ffffff" className="drop-shadow-[0_0_5px_#ffffff]" />

      <path
        d="M 175 352 L 150 378 L 125 378 L 105 398 L 105 420"
        stroke="rgba(56, 189, 248, 0.45)"
        strokeWidth="1.2"
      />
      <circle cx="105" cy="422" r="2" fill="#38bdf8" />

      <path
        d="M 275 352 L 300 378 L 325 378 L 345 398 L 345 420"
        stroke="rgba(56, 189, 248, 0.45)"
        strokeWidth="1.2"
      />
      <circle cx="345" cy="422" r="2" fill="#38bdf8" />

      {/* LEFT Circuit Traces */}
      <path
        d="M 75 225 L 12 225"
        stroke="rgba(56, 189, 248, 0.7)"
        strokeWidth="1.6"
        className="circuit-pulse-line"
      />
      <circle cx="12" cy="225" r="3.2" fill="#38bdf8" className="drop-shadow-[0_0_8px_#38bdf8]" />

      <path
        d="M 85 195 L 50 195 L 30 175 L 12 175"
        stroke="rgba(56, 189, 248, 0.5)"
        strokeWidth="1.3"
      />
      <circle cx="10" cy="175" r="2.5" fill="#ffffff" />

      <path
        d="M 85 255 L 50 255 L 30 275 L 12 275"
        stroke="rgba(56, 189, 248, 0.5)"
        strokeWidth="1.3"
      />
      <circle cx="10" cy="275" r="2.5" fill="#ffffff" />

      {/* RIGHT Circuit Traces */}
      <path
        d="M 375 225 L 438 225"
        stroke="rgba(56, 189, 248, 0.7)"
        strokeWidth="1.6"
        className="circuit-pulse-line"
      />
      <circle cx="438" cy="225" r="3.2" fill="#38bdf8" className="drop-shadow-[0_0_8px_#38bdf8]" />

      <path
        d="M 365 195 L 400 195 L 420 175 L 438 175"
        stroke="rgba(56, 189, 248, 0.5)"
        strokeWidth="1.3"
      />
      <circle cx="440" cy="175" r="2.5" fill="#ffffff" />

      <path
        d="M 365 255 L 400 255 L 420 275 L 438 275"
        stroke="rgba(56, 189, 248, 0.5)"
        strokeWidth="1.3"
      />
      <circle cx="440" cy="275" r="2.5" fill="#ffffff" />
    </svg>
  );
}
