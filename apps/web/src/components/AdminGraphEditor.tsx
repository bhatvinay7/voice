"use client";

import React, { useState } from "react";
import {
  useAdminGraphNodes,
  useAdminAddGraphNode,
  useAdminUpdateGraphNode,
  useAdminDeleteGraphNode,
} from "../lib/api";
import { AdminGraphNode } from "../../types/graph";
import { Workflow, ArrowRight, ShieldCheck, Database, FileText, Award, CalendarCheck, FileCheck, Plus, Trash2, Edit3, Check, X } from "lucide-react";

export function AdminGraphEditor(): React.ReactElement {
  const { data, isLoading } = useAdminGraphNodes();
  const addNodeMutation = useAdminAddGraphNode();
  const updateNodeMutation = useAdminUpdateGraphNode();
  const deleteNodeMutation = useAdminDeleteGraphNode();

  const [stepType, setStepType] = useState("DubaiPurchaseProcedureGuide");
  const [instruction, setInstruction] = useState("");
  const [editingNodeId, setEditingNodeId] = useState<string | null>(null);
  const [editInstruction, setEditInstruction] = useState("");

  const stepTypeDescriptions: Record<string, string> = {
    UserInputVerifier: "Verify caller identity, sanitize prompts against command injection, and check Emirates ID / KYC status",
    RagFetchAndAgentAnalysis: "Execute semantic RAG retrieval across 22 UAE property listings & DLD regulations with LLM reasoning",
    DubaiPurchaseProcedureGuide: "Guide buyer through Form A/B, Unified Form F (MOU) signing, 10% escrow deposit cheque, developer NOC, and 4% DLD transfer fee",
    GoldenVisaEligibilityCheck: "Evaluate UAE 10-Year Golden Visa qualification for freehold property purchases >= AED 2,000,000",
    SlotBookingAndOutbox: "Execute atomic Redis viewing slot lock, commit PostgreSQL booking record, and dispatch notification outbox",
    ContractFilingAndTrustee: "Generate Unified DLD Tenancy Contract or Form F sale draft and route to admin compliance review queue",
    DynamicErrorResolver: "Autonomous fallback recovery and human agent escalation",
  };

  const handleStepTypeChange = (newType: string) => {
    setStepType(newType);
    if (!instruction || Object.values(stepTypeDescriptions).includes(instruction)) {
      setInstruction(stepTypeDescriptions[newType] || "");
    }
  };

  const handleAddNode = (e: React.FormEvent): void => {
    e.preventDefault();
    if (!instruction) return;

    const newNode: AdminGraphNode = {
      id: `node-${Date.now()}`,
      step_type: stepType,
      instruction,
      status: "Active",
      is_preset: false,
    };

    addNodeMutation.mutate(newNode, {
      onSuccess: () => {
        setInstruction("");
      },
    });
  };

  const startEditing = (node: AdminGraphNode) => {
    setEditingNodeId(node.id);
    setEditInstruction(node.instruction);
  };

  const saveEdit = (id: string) => {
    if (!editInstruction) return;
    updateNodeMutation.mutate(
      { id, instruction: editInstruction },
      {
        onSuccess: () => setEditingNodeId(null),
      }
    );
  };

  const getStepIcon = (type: string) => {
    switch (type) {
      case "UserInputVerifier":
        return <ShieldCheck className="w-4 h-4 text-cyan-400" />;
      case "RagFetchAndAgentAnalysis":
        return <Database className="w-4 h-4 text-purple-400" />;
      case "DubaiPurchaseProcedureGuide":
        return <FileText className="w-4 h-4 text-amber-400" />;
      case "GoldenVisaEligibilityCheck":
        return <Award className="w-4 h-4 text-emerald-400" />;
      case "SlotBookingAndOutbox":
        return <CalendarCheck className="w-4 h-4 text-blue-400" />;
      case "ContractFilingAndTrustee":
        return <FileCheck className="w-4 h-4 text-rose-400" />;
      default:
        return <Workflow className="w-4 h-4 text-theme-sub" />;
    }
  };

  return (
    <div className="bg-theme-card border border-theme-border rounded-2xl p-6 space-y-6 transition-colors duration-300">
      <div>
        <div className="flex items-center space-x-2">
          <Workflow className="w-5 h-5 text-cyan-400" />
          <h3 className="text-xl font-bold text-theme-text font-serif">
            Dubai Real Estate Workflow DAG Editor
          </h3>
        </div>
        <p className="text-xs text-theme-sub mt-1">
          Graph database (Memgraph / Neo4j) static state machine defining prompt instructions, statutory Dubai purchase procedures (Form F, NOC, DLD 4%, Golden Visa), and autonomous tool execution DAGs.
        </p>
      </div>

      {/* Add New DAG Node Form */}
      <form
        onSubmit={handleAddNode}
        className="grid grid-cols-1 md:grid-cols-3 gap-4 bg-white dark:bg-black p-4 rounded-xl border border-slate-300 dark:border-white/20"
      >
        <div>
          <label className="block text-xs font-semibold text-black dark:text-white mb-1">
            Workflow Step Type
          </label>
          <select
            value={stepType}
            onChange={(e) => handleStepTypeChange(e.target.value)}
            className="w-full bg-white dark:bg-black border border-slate-300 dark:border-white/20 text-black dark:text-white rounded-lg px-3 py-2 text-xs focus:outline-none focus:border-cyan-500"
          >
            <option value="UserInputVerifier">1. UserInputVerifier (KYC / Sanitize)</option>
            <option value="RagFetchAndAgentAnalysis">2. RagFetchAndAgentAnalysis (Semantic RAG)</option>
            <option value="DubaiPurchaseProcedureGuide">3. DubaiPurchaseProcedureGuide (Form F / DLD)</option>
            <option value="GoldenVisaEligibilityCheck">4. GoldenVisaEligibilityCheck (AED 2M+)</option>
            <option value="SlotBookingAndOutbox">5. SlotBookingAndOutbox (Redis Lock)</option>
            <option value="ContractFilingAndTrustee">6. ContractFilingAndTrustee (Audit Queue)</option>
            <option value="DynamicErrorResolver">7. DynamicErrorResolver (Recovery)</option>
          </select>
        </div>
        <div className="md:col-span-2 flex items-end space-x-3">
          <div className="flex-1">
            <label className="block text-xs font-semibold text-black dark:text-white mb-1">
              Workflow Instruction Text
            </label>
            <input
              type="text"
              placeholder="e.g. Guide buyer through Form F (MOU), developer NOC, and 4% DLD fee..."
              value={instruction}
              onChange={(e) => setInstruction(e.target.value)}
              className="w-full bg-white dark:bg-black border border-slate-300 dark:border-white/20 text-black dark:text-white rounded-lg px-3 py-2 text-xs focus:outline-none focus:border-cyan-500 placeholder:text-slate-400 dark:placeholder:text-zinc-500"
            />
          </div>
          <button
            type="submit"
            disabled={addNodeMutation.isPending}
            className="flex items-center space-x-1.5 bg-white hover:bg-zinc-100 text-black dark:bg-black dark:text-white dark:hover:bg-zinc-900 dark:border-white/20 px-5 py-2 rounded-lg text-xs font-bold transition border border-slate-300 shrink-0"
          >
            <Plus className="w-3.5 h-3.5" />
            <span>{addNodeMutation.isPending ? "Adding..." : "Add Node"}</span>
          </button>
        </div>
      </form>

      {/* DAG Workflow Graph Nodes List */}
      {isLoading ? (
        <p className="text-xs text-theme-sub">Loading DAG workflow nodes...</p>
      ) : (
        <div className="space-y-4">
          <div className="flex justify-between items-center">
            <p className="text-xs font-bold text-theme-sub uppercase tracking-wider">
              Active Graph Pipeline Steps ({data?.graph_nodes.length ?? 0} Nodes)
            </p>
            <span className="text-[10px] text-cyan-400 font-mono">
              Deterministic DAG • Topologically Sorted
            </span>
          </div>

          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
            {data?.graph_nodes.map((node, index) => (
              <div
                key={node.id}
                className="bg-theme-main p-4 rounded-xl border border-theme-border relative space-y-3 group hover:border-cyan-500/40 transition-all flex flex-col justify-between"
              >
                <div>
                  <div className="flex justify-between items-center mb-2">
                    <div className="flex items-center space-x-1.5">
                      {getStepIcon(node.step_type)}
                      <span className="text-[10px] font-mono font-bold bg-cyan-500/10 text-cyan-400 px-2 py-0.5 rounded border border-cyan-500/30">
                        Step {index + 1}: {node.step_type}
                      </span>
                    </div>
                    <div className="flex items-center gap-1.5">
                      <span className="text-[10px] text-emerald-400 font-semibold">{node.status}</span>
                      {!node.is_preset && (
                        <button
                          onClick={() => deleteNodeMutation.mutate(node.id)}
                          className="opacity-0 group-hover:opacity-100 text-red-500 hover:text-red-400 transition-opacity p-1"
                          title="Delete Node"
                        >
                          <Trash2 className="w-3.5 h-3.5" />
                        </button>
                      )}
                    </div>
                  </div>

                  {editingNodeId === node.id ? (
                    <div className="mt-2 space-y-2">
                      <textarea
                        rows={3}
                        value={editInstruction}
                        onChange={(e) => setEditInstruction(e.target.value)}
                        className="w-full bg-white dark:bg-black border border-slate-300 dark:border-white/20 text-black dark:text-white rounded px-2.5 py-1.5 text-xs focus:outline-none focus:border-cyan-500"
                      />
                      <div className="flex gap-2">
                        <button
                          onClick={() => saveEdit(node.id)}
                          className="flex items-center gap-1 text-xs bg-emerald-600 hover:bg-emerald-500 px-2.5 py-1 rounded font-bold text-white"
                        >
                          <Check className="w-3 h-3" /> Save
                        </button>
                        <button
                          onClick={() => setEditingNodeId(null)}
                          className="flex items-center gap-1 text-xs bg-zinc-700 hover:bg-zinc-600 px-2.5 py-1 rounded font-bold text-white"
                        >
                          <X className="w-3 h-3" /> Cancel
                        </button>
                      </div>
                    </div>
                  ) : (
                    <div
                      className="cursor-pointer group/text"
                      onClick={() => startEditing(node)}
                    >
                      <p className="text-xs text-theme-text font-medium leading-relaxed hover:text-cyan-300 transition-colors">
                        {node.instruction}
                      </p>
                      <span className="text-[10px] text-theme-sub opacity-0 group-hover/text:opacity-100 transition-opacity flex items-center gap-1 mt-1 text-cyan-400">
                        <Edit3 className="w-2.5 h-2.5" /> Click to edit instruction
                      </span>
                    </div>
                  )}
                </div>

                <div className="flex justify-between items-center pt-2 border-t border-theme-border/50 text-[10px] text-theme-sub">
                  <span>{node.is_preset ? "Statutory Preset" : "Custom Step"}</span>
                  {index < (data.graph_nodes.length - 1) && (
                    <span className="flex items-center text-cyan-400 font-mono">
                      <span>Next</span> <ArrowRight className="w-2.5 h-2.5 ml-0.5" />
                    </span>
                  )}
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
