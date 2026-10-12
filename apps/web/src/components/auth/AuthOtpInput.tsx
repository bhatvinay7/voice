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
      <label className="text-xs font-bold text-black dark:text-white uppercase tracking-wider pl-1">
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
            className="w-12 h-14 bg-white dark:bg-black border border-slate-300 dark:border-white/20 text-center text-xl font-bold text-black dark:text-white rounded-2xl focus:outline-none focus:border-black dark:focus:border-white transition-all shadow-inner"
          />
        ))}
      </div>
    </div>
  );
}
