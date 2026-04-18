"use client";

import { useEffect, useState, useMemo } from "react";
import Link from "next/link";
import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  CardDescription,
} from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";

// ---------- Types ----------

interface ModelEntry {
  name: string;
  license: string;
  node_count: number;
  avg_latency_ms: number | null;
  uptime_7d: number | null;
  price_per_m_tokens: number;
}

interface ModelBrowserResponse {
  models: ModelEntry[];
  price_per_m_tokens: number;
}

type SortKey = "name" | "latency" | "uptime" | "price";

// ---------- Page ----------

export default function ModelsPage() {
  const [data, setData] = useState<ModelBrowserResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const [search, setSearch] = useState("");
  const [minUptime, setMinUptime] = useState("");
  const [sortKey, setSortKey] = useState<SortKey>("name");
  const [sortAsc, setSortAsc] = useState(true);

  useEffect(() => {
    fetch("/api/models/stats")
      .then((r) => r.json())
      .then((d: ModelBrowserResponse) => setData(d))
      .catch(() => setError("Could not reach the gateway."))
      .finally(() => setLoading(false));
  }, []);

  const filtered = useMemo(() => {
    if (!data) return [];
    let list = [...data.models];

    if (search.trim()) {
      const q = search.trim().toLowerCase();
      list = list.filter((m) => m.name.toLowerCase().includes(q));
    }

    if (minUptime.trim()) {
      const threshold = parseFloat(minUptime) / 100;
      if (!isNaN(threshold)) {
        list = list.filter(
          (m) => m.uptime_7d !== null && m.uptime_7d >= threshold
        );
      }
    }

    list.sort((a, b) => {
      let cmp = 0;
      if (sortKey === "name") {
        cmp = a.name.localeCompare(b.name);
      } else if (sortKey === "latency") {
        const la = a.avg_latency_ms ?? Infinity;
        const lb = b.avg_latency_ms ?? Infinity;
        cmp = la - lb;
      } else if (sortKey === "uptime") {
        const ua = a.uptime_7d ?? -1;
        const ub = b.uptime_7d ?? -1;
        cmp = ub - ua;
      } else if (sortKey === "price") {
        cmp = a.price_per_m_tokens - b.price_per_m_tokens;
      }
      return sortAsc ? cmp : -cmp;
    });

    return list;
  }, [data, search, minUptime, sortKey, sortAsc]);

  function toggleSort(key: SortKey) {
    if (sortKey === key) {
      setSortAsc((prev) => !prev);
    } else {
      setSortKey(key);
      setSortAsc(true);
    }
  }

  return (
    <main className="flex flex-col min-h-screen">
      <nav className="border-b border-border px-6 py-4 flex items-center justify-between">
        <Link
          href="/"
          className="font-mono text-sm font-semibold tracking-tight hover:text-muted-foreground transition-colors"
        >
          infer.ram4.dev
        </Link>
        <div className="flex items-center gap-4 text-sm text-muted-foreground">
          <Link href="/status" className="hover:text-foreground transition-colors text-xs">
            Status
          </Link>
          <Link href="/keys" className="hover:text-foreground transition-colors text-xs">
            API Keys
          </Link>
          <Link
            href="/models"
            className="text-foreground font-medium text-xs font-mono"
          >
            Models
          </Link>
          <Link href="/dashboard/consumer" className="hover:text-foreground transition-colors text-xs">
            Usage
          </Link>
          <Link href="/provider" className="hover:text-foreground transition-colors text-xs">
            Provider
          </Link>
        </div>
      </nav>

      <div className="flex-1 px-6 py-10 max-w-6xl mx-auto w-full space-y-8">
        <div>
          <h1 className="text-xl font-bold font-mono">Model browser</h1>
          <p className="text-sm text-muted-foreground mt-1">
            All active models on the network — latency and uptime from live
            health probes (last 7 days).
          </p>
        </div>

        {/* Filters */}
        <div className="flex flex-col sm:flex-row gap-3">
          <input
            type="text"
            placeholder="Search models…"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="flex-1 h-8 rounded-lg border border-input bg-transparent px-3 text-sm outline-none focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/30 placeholder:text-muted-foreground"
          />
          <input
            type="number"
            placeholder="Min uptime %"
            value={minUptime}
            onChange={(e) => setMinUptime(e.target.value)}
            min={0}
            max={100}
            className="w-36 h-8 rounded-lg border border-input bg-transparent px-3 text-sm outline-none focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/30 placeholder:text-muted-foreground"
          />
          <div className="flex items-center gap-2 text-xs text-muted-foreground">
            <span>Sort:</span>
            {(["name", "latency", "uptime", "price"] as SortKey[]).map((k) => (
              <button
                key={k}
                onClick={() => toggleSort(k)}
                className={`px-2 py-1 rounded border text-xs font-mono transition-colors ${
                  sortKey === k
                    ? "border-primary text-foreground"
                    : "border-border hover:border-muted-foreground"
                }`}
              >
                {k}
                {sortKey === k ? (sortAsc ? " ↑" : " ↓") : ""}
              </button>
            ))}
          </div>
        </div>

        {/* Error */}
        {error && (
          <div className="rounded-lg bg-destructive/10 border border-destructive/30 px-4 py-3 text-sm text-destructive">
            {error}
          </div>
        )}

        {/* Model cards */}
        {loading ? (
          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
            {[0, 1, 2, 3, 4, 5].map((i) => (
              <Card key={i}>
                <CardHeader className="pb-2">
                  <Skeleton className="h-4 w-32" />
                  <Skeleton className="h-3 w-20 mt-1" />
                </CardHeader>
                <CardContent className="space-y-2">
                  <Skeleton className="h-3 w-full" />
                  <Skeleton className="h-3 w-3/4" />
                </CardContent>
              </Card>
            ))}
          </div>
        ) : filtered.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            {data?.models.length === 0
              ? "No models registered. Connect a GPU node to add models to the network."
              : "No models match the current filters."}
          </p>
        ) : (
          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
            {filtered.map((model) => (
              <ModelCard key={model.name} model={model} />
            ))}
          </div>
        )}

        {!loading && data && (
          <p className="text-xs text-muted-foreground">
            {filtered.length} of {data.models.length} models shown.
          </p>
        )}
      </div>
    </main>
  );
}

// ---------- Model card ----------

function ModelCard({ model }: { model: ModelEntry }) {
  return (
    <Card className="flex flex-col">
      <CardHeader className="pb-2">
        <CardTitle className="text-sm font-mono leading-tight">
          {model.name}
        </CardTitle>
        <CardDescription className="text-xs">
          {model.node_count} node{model.node_count !== 1 ? "s" : ""}
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-3 flex-1">
        <div className="grid grid-cols-2 gap-2 text-xs">
          <Stat label="Price" value={`$${model.price_per_m_tokens.toFixed(2)}/M tok`} />
          <Stat
            label="Avg latency"
            value={
              model.avg_latency_ms != null
                ? `${model.avg_latency_ms.toFixed(0)} ms`
                : "—"
            }
          />
          <Stat
            label="Uptime 7d"
            value={
              model.uptime_7d != null
                ? `${(model.uptime_7d * 100).toFixed(1)}%`
                : "—"
            }
            valueClass={uptimeColor(model.uptime_7d)}
          />
          <Stat label="License" value={model.license} />
        </div>

        <div className="mt-auto pt-1">
          <LicenseBadge license={model.license} />
        </div>
      </CardContent>
    </Card>
  );
}

function Stat({
  label,
  value,
  valueClass,
}: {
  label: string;
  value: string;
  valueClass?: string;
}) {
  return (
    <div>
      <p className="text-muted-foreground">{label}</p>
      <p className={`font-mono font-medium ${valueClass ?? ""}`}>{value}</p>
    </div>
  );
}

function LicenseBadge({ license }: { license: string }) {
  const lower = license.toLowerCase();
  const isPermissive =
    lower.includes("mit") ||
    lower.includes("apache") ||
    lower.includes("bsd") ||
    lower.includes("cc");

  return (
    <Badge
      variant={isPermissive ? "default" : "secondary"}
      className="text-xs font-mono"
    >
      {license}
    </Badge>
  );
}

// ---------- Helpers ----------

function uptimeColor(uptime: number | null): string {
  if (uptime === null) return "text-muted-foreground";
  if (uptime >= 0.99) return "text-green-500";
  if (uptime >= 0.95) return "text-yellow-500";
  return "text-red-500";
}
