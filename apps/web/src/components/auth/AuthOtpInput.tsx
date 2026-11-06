"use client";

import React from "react";

interface AuthOtpInputProps {
  otpCode: string[];
  onChangeOtp: (index: number, val: string) => void;
}

export function AuthOtpInput({
  otpCode,
  onChangeOtp,
}: AuthOtpInputProps): React.ReactElement {
  return (
    <div className="space-y-2">
      <label className="text-xs font-bold text-theme-text uppercase tracking-wider pl-1">
        6-Digit Verification Code
      </label>
      <div className="flex items-center justify-between gap-2">
        {otpCode.map((digit, idx) => (
          <input
            key={idx}
            id={`otp-input-${idx}`}
            type="text"
            inputMode="numeric"
            maxLength={1}
            value={digit}
            onChange={(e) => onChangeOtp(idx, e.target.value)}
            className="w-12 h-14 bg-theme-card border border-theme-border text-center text-xl font-bold text-theme-text rounded-2xl focus:outline-none focus:border-cyan-500 transition-all shadow-inner"
          />
        ))}
      </div>
    </div>
  );
}
