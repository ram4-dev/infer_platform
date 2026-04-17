import { Suspense } from "react";
import Link from "next/link";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Skeleton } from "@/components/ui/skeleton";
import { Separator } from "@/components/ui/separator";

// ---------- Types ----------

interface NodeProviderStats {
  node_id: string;
  node_name: string;
  gpu_name: string;
  vram_mb: number;
  status: string;
  uptime_pct_7d: number;
  avg_latency_ms_7d: number | null;
  probe_count_7d: number;
  request_count_7d: number;
  tokens_in_7d: number;
  tokens_out_7d: number;
  tokens_served_7d: number;
  estimated_earnings_usd_7d: number;
  stripe_onboarding_complete: boolean;
  models: string[];
}

interface ProviderTotals {
  node_count: number;
  request_count_7d: number;
  tokens_served_7d: number;
  estimated_earnings_usd_7d: number;
}

interface ProviderStatsResponse {
  nodes: NodeProviderStats[];
  totals: ProviderTotals;
}

// ---------- Data fetcher ----------

async function fetchProviderStats(): Promise<ProviderStatsResponse> {
  const base =
    process.env.NEXT_PUBLIC_SITE_URL ??
    process.env.VERCEL_URL ??
    "http://localhost:3000";
  const url = `${base.startsWith("http") ? base : `https://${base}`}/api/provider`;

  try {
    const res = await fetch(url, { cache: "no-store" });
    if (!res.ok) return emptyStats();
    return res.json();
  } catch {
    return emptyStats();
  }
}

function emptyStats(): ProviderStatsResponse {
  return {
    nodes: [],
    totals: {
      node_count: 0,
      request_count_7d: 0,
      tokens_served_7d: 0,
      estimated_earnings_usd_7d: 0,
    },
  };
}

// ---------- Page ----------

export default function ProviderPage() {
  return (
    <main className="flex flex-col min-h-screen">
      {/* Nav */}
      <nav className="border-b border-border px-6 py-4 flex items-center justify-between">
        <Link
          href="/"
          className="font-mono text-sm font-semibold tracking-tight hover:text-muted-foreground transition-colors"
        >
          infer.ram4.dev
        </Link>
        <div className="flex items-center gap-4 text-sm text-muted-foreground">
          <Link
            href="/status"
            className="hover:text-foreground transition-colors text-xs"
          >
            Status
          </Link>
          <Link
            href="/keys"
            className="hover:text-foreground transition-colors text-xs"
          >
            API Keys
          </Link>
          <Link
            href="/provider"
            className="text-foreground font-medium text-xs font-mono"
          >
            Provider
          </Link>
        </div>
      </nav>

      <div className="flex-1 px-6 py-10 max-w-6xl mx-auto w-full space-y-10">
        <div>
          <h1 className="text-xl font-bold font-mono">Provider dashboard</h1>
          <p className="text-sm text-muted-foreground mt-1">
            Earnings, uptime, and request analytics for the last 7 days.
          </p>
        </div>

        {/* Summary cards */}
        <Suspense fallback={<SummaryCardsSkeleton />}>
          <SummaryCards />
        </Suspense>

        <Separator />

        {/* Per-node table */}
        <section>
          <h2 className="text-base font-semibold mb-4">Node analytics</h2>
          <Suspense fallback={<TableSkeleton />}>
            <NodesTable />
          </Suspense>
        </section>

        <Separator />

        {/* Notes */}
        <p className="text-xs text-muted-foreground">
          Earnings are estimated at $0.000001/token × 70% revenue share.
          Actual payouts are processed via Stripe Connect and may differ.
          Tokens are attributed to nodes via model association — nodes serving
          the same model share attribution.
        </p>
      </div>
    </main>
  );
}

// ---------- Dynamic components ----------

async function SummaryCards() {
  const { totals } = await fetchProviderStats();

  return (
    <div className="grid grid-cols-1 sm:grid-cols-4 gap-4">
      <StatCard
        title="Active nodes"
        value={String(totals.node_count)}
        sub="Registered GPU providers"
      />
      <StatCard
        title="Requests (7d)"
        value={totals.request_count_7d.toLocaleString()}
        sub="Inference requests served"
      />
      <StatCard
        title="Tokens served (7d)"
        value={formatTokens(totals.tokens_served_7d)}
        sub="Input + output tokens"
      />
      <StatCard
        title="Est. earnings (7d)"
        value={`$${totals.estimated_earnings_usd_7d.toFixed(4)}`}
        sub="At $0.000001/token × 70%"
      />
    </div>
  );
}

async function NodesTable() {
  const { nodes } = await fetchProviderStats();

  if (nodes.length === 0) {
    return (
      <p className="text-sm text-muted-foreground">
        No nodes registered. Run{" "}
        <code className="font-mono text-xs">infer connect</code> on a GPU
        machine to join the network.
      </p>
    );
  }

  return (
    <div className="overflow-x-auto">
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>Node</TableHead>
            <TableHead>GPU</TableHead>
            <TableHead>Status</TableHead>
            <TableHead>Uptime 7d</TableHead>
            <TableHead>Avg latency</TableHead>
            <TableHead>Requests 7d</TableHead>
            <TableHead>Tokens 7d</TableHead>
            <TableHead>Est. earnings 7d</TableHead>
            <TableHead>Stripe</TableHead>
            <TableHead>Models</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {nodes.map((node) => (
            <TableRow key={node.node_id}>
              <TableCell className="font-mono text-xs">
                {node.node_name}
              </TableCell>
              <TableCell className="text-xs">
                <span title={`${(node.vram_mb / 1024).toFixed(1)} GB VRAM`}>
                  {node.gpu_name}
                </span>
              </TableCell>
              <TableCell>
                <NodeStatusBadge status={node.status} />
              </TableCell>
              <TableCell className="font-mono text-xs">
                <UptimeBadge pct={node.uptime_pct_7d} probes={node.probe_count_7d} />
              </TableCell>
              <TableCell className="font-mono text-xs text-muted-foreground">
                {node.avg_latency_ms_7d != null
                  ? `${node.avg_latency_ms_7d.toFixed(0)} ms`
                  : "—"}
              </TableCell>
              <TableCell className="font-mono text-xs">
                {node.request_count_7d.toLocaleString()}
              </TableCell>
              <TableCell className="font-mono text-xs">
                {formatTokens(node.tokens_served_7d)}
              </TableCell>
              <TableCell className="font-mono text-xs">
                ${node.estimated_earnings_usd_7d.toFixed(4)}
              </TableCell>
              <TableCell>
                {node.stripe_onboarding_complete ? (
                  <Badge variant="default" className="text-xs font-mono">
                    connected
                  </Badge>
                ) : (
                  <Badge variant="secondary" className="text-xs font-mono">
                    pending
                  </Badge>
                )}
              </TableCell>
              <TableCell className="text-xs">
                <div className="flex flex-wrap gap-1">
                  {node.models.length === 0 ? (
                    <span className="text-muted-foreground">—</span>
                  ) : (
                    node.models.map((m) => (
                      <Badge
                        key={m}
                        variant="outline"
                        className="font-mono text-xs px-1.5 py-0"
                      >
                        {m}
                      </Badge>
                    ))
                  )}
                </div>
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </div>
  );
}

// ---------- Helper components ----------

function StatCard({
  title,
  value,
  sub,
}: {
  title: string;
  value: string;
  sub: string;
}) {
  return (
    <Card>
      <CardHeader className="pb-1">
        <CardTitle className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
          {title}
        </CardTitle>
      </CardHeader>
      <CardContent>
        <p className="text-3xl font-bold font-mono">{value}</p>
        <p className="text-xs text-muted-foreground mt-1">{sub}</p>
      </CardContent>
    </Card>
  );
}

function NodeStatusBadge({ status }: { status: string }) {
  const variant =
    status === "online"
      ? "default"
      : status === "busy"
        ? "secondary"
        : "destructive";
  return (
    <Badge variant={variant} className="text-xs font-mono">
      {status}
    </Badge>
  );
}

function UptimeBadge({ pct, probes }: { pct: number; probes: number }) {
  if (probes === 0) {
    return <span className="text-muted-foreground">no data</span>;
  }
  const color =
    pct >= 99
      ? "text-green-500"
      : pct >= 95
        ? "text-yellow-500"
        : "text-red-500";
  return <span className={color}>{pct.toFixed(2)}%</span>;
}

// ---------- Formatters ----------

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(2)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

// ---------- Skeleton fallbacks ----------

function SummaryCardsSkeleton() {
  return (
    <div className="grid grid-cols-1 sm:grid-cols-4 gap-4">
      {[0, 1, 2, 3].map((i) => (
        <Card key={i}>
          <CardHeader className="pb-1">
            <Skeleton className="h-3 w-24" />
          </CardHeader>
          <CardContent>
            <Skeleton className="h-8 w-16 mb-2" />
            <Skeleton className="h-3 w-32" />
          </CardContent>
        </Card>
      ))}
    </div>
  );
}

function TableSkeleton() {
  return (
    <div className="space-y-2">
      {[0, 1, 2].map((i) => (
        <Skeleton key={i} className="h-10 w-full" />
      ))}
    </div>
  );
}
