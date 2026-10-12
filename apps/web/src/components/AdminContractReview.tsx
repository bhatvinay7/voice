"use client";

import React, { JSX } from "react";
import { useContractReviewQueue, useApproveContract } from "../lib/api";
import { formatCurrency, getContractRent } from "../lib/formatters";

export function AdminContractReview(): React.ReactElement {
  const { data, isLoading } = useContractReviewQueue();
  const approveMutation = useApproveContract();

  const handleApprove = (contractId: string): void => {
    approveMutation.mutate(contractId);
  };

  return (
    <div className="bg-theme-card border border-theme-border rounded-2xl p-6 space-y-6 transition-colors duration-300">
      <div className="flex justify-between items-center">
        <div>
          <h3 className="text-xl font-bold text-theme-text font-serif">Tenancy Contract Human Review Queue</h3>
          <p className="text-xs text-theme-sub">
            Low-confidence extracted contracts (&lt; 0.85) pending human verification
          </p>
        </div>
        <span className="text-xs bg-cyan-500/10 text-cyan-400 px-3 py-1 rounded-full border border-cyan-500/30 font-semibold">
          Pending Approvals: {data?.review_queue.length || 0}
        </span>
      </div>

      {isLoading ? (
        <p className="text-xs text-theme-sub">Loading review queue...</p>
      ) : data?.review_queue.length === 0 ? (
        <div className="p-8 text-center bg-theme-main rounded-xl border border-theme-border text-theme-sub text-xs font-medium">
          ✅ No low-confidence tenancy contracts in review queue. All contracts approved!
        </div>
      ) : (
        <div className="space-y-4">
          {data?.review_queue.map((contract) => (
            <div
              key={contract.id}
              className="bg-theme-main p-5 rounded-xl border border-cyan-500/30 space-y-3"
            >
              <div className="flex justify-between items-start">
                <div>
                  <h4 className="font-bold text-theme-text text-sm">{contract.filename}</h4>
                  <p className="text-xs text-theme-sub">{contract.property_address}</p>
                </div>
                <span className="text-xs font-mono font-bold text-cyan-400 bg-cyan-500/10 px-2.5 py-1 rounded border border-cyan-500/30">
                  Confidence Score: {((contract.confidence_score ?? 0) * 100).toFixed(0)}%
                </span>
              </div>

              <div className="grid grid-cols-2 md:grid-cols-4 gap-3 text-xs bg-theme-card p-3 rounded-lg border border-theme-border">
                <div>
                  <span className="text-theme-sub block text-[10px] uppercase">Landlord</span>
                  <span className="text-theme-text font-medium">{contract.landlord_name || "—"}</span>
                </div>
                <div>
                  <span className="text-theme-sub block text-[10px] uppercase">Tenant</span>
                  <span className="text-theme-text font-medium">{contract.tenant_name || "—"}</span>
                </div>
                <div>
                  <span className="text-theme-sub block text-[10px] uppercase">Annual Rent</span>
                  <span className="text-emerald-400 font-bold">{formatCurrency(getContractRent(contract), { currency: "$" })}</span>
                </div>
                <div>
                  <span className="text-theme-sub block text-[10px] uppercase">Notice Period</span>
                  <span className="text-theme-text font-medium">{contract.notice_period_days ?? 0} Days</span>
                </div>
              </div>

              <div className="p-3 bg-theme-card rounded-lg text-xs text-theme-text border border-theme-border">
                <span className="font-semibold text-cyan-400 block mb-1">Clause Summary:</span>
                {contract.clause_summary}
              </div>

              <div className="flex justify-between items-center pt-2">
                <span className="text-[10px] font-mono text-theme-sub">R2 Storage: {contract.r2_vault_path}</span>
                <button
                  onClick={() => handleApprove(contract.id)}
                  disabled={approveMutation.isPending}
                  className="bg-emerald-600 hover:bg-emerald-500 text-white font-bold text-xs px-4 py-2 rounded-lg transition"
                >
                  {approveMutation.isPending ? "Approving..." : "Approve Extracted Fields"}
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
};
