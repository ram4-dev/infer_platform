import { Suspense } from "react";
import Link from "next/link";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
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

interface ModelBreakdown {
  model: string;
  requests: number;
  tokens_in: number;
  tokens_out: number;
  tokens_total: number;
  spend_usd: number;
}

interface DailyPoint {
  date: string;
  requests: number;
  tokens: number;
  spend_usd: number;
}

interface ConsumerAnalytics {
  total_requests: number;
  total_tokens_in: number;
  total_tokens_out: number;
  total_tokens: number;
  total_spend_usd: number;
  tokens_by_model: ModelBreakdown[];
  daily_spend: DailyPoint[];
}

// ---------- Data fetcher ----------

async function fetchAnalytics(): Promise<ConsumerAnalytics> {
  const base =
    process.env.NEXT_PUBLIC_SITE_URL ??
    process.env.VERCEL_URL ??
    "http://localhost:3000";
  const url = `${base.startsWith("http") ? base : `https://${base}`}/api/consumer`;

  try {
    const res = await fetch(url, { cache: "no-store" });
    if (!res.ok) return emptyAnalytics();
    return res.json();
  } catch {
    return emptyAnalytics();
  }
}

function emptyAnalytics(): ConsumerAnalytics {
  return {
    total_requests: 0,
    total_tokens_in: 0,
    total_tokens_out: 0,
    total_tokens: 0,
    total_spend_usd: 0,
    tokens_by_model: [],
    daily_spend: [],
  };
}

// ---------- Page ----------

export default function ConsumerDashboardPage() {
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
          <Link href="/models" className="hover:text-foreground transition-colors text-xs">
            Models
          </Link>
          <Link
            href="/dashboard/consumer"
            className="text-foreground font-medium text-xs font-mono"
          >
            Usage
          </Link>
          <Link href="/provider" className="hover:text-foreground transition-colors text-xs">
            Provider
          </Link>
        </div>
      </nav>

      <div className="flex-1 px-6 py-10 max-w-6xl mx-auto w-full space-y-10">
        <div>
          <h1 className="text-xl font-bold font-mono">Consumer dashboard</h1>
          <p className="text-sm text-muted-foreground mt-1">
            Spend, token usage, and model breakdown for the last 30 days.
          </p>
        </div>

        <Suspense fallback={<SummaryCardsSkeleton />}>
          <SummaryCards />
        </Suspense>

        <Separator />

        <section>
          <h2 className="text-base font-semibold mb-4">Usage by model</h2>
          <Suspense fallback={<TableSkeleton />}>
            <ModelBreakdownTable />
          </Suspense>
        </section>

        <Separator />

        <section>
          <h2 className="text-base font-semibold mb-4">Daily spend (last 30 days)</h2>
          <Suspense fallback={<TableSkeleton />}>
            <DailySpendChart />
          </Suspense>
        </section>

        <p className="text-xs text-muted-foreground">
          Spend is estimated at $0.000001/token. Actual invoices are issued via
          Stripe at the end of the billing period.
        </p>
      </div>
    </main>
  );
}

// ---------- Dynamic components ----------

async function SummaryCards() {
  const data = await fetchAnalytics();

  return (
    <div className="grid grid-cols-1 sm:grid-cols-4 gap-4">
      <StatCard
        title="Requests (30d)"
        value={data.total_requests.toLocaleString()}
        sub="API calls made"
      />
      <StatCard
        title="Tokens in (30d)"
        value={formatTokens(data.total_tokens_in)}
        sub="Prompt tokens"
      />
      <StatCard
        title="Tokens out (30d)"
        value={formatTokens(data.total_tokens_out)}
        sub="Completion tokens"
      />
      <StatCard
        title="Est. spend (30d)"
        value={`$${data.total_spend_usd.toFixed(4)}`}
        sub="At $0.000001/token"
      />
    </div>
  );
}

async function ModelBreakdownTable() {
  const { tokens_by_model } = await fetchAnalytics();

  if (tokens_by_model.length === 0) {
    return (
      <p className="text-sm text-muted-foreground">
        No usage data yet. Make a request to{" "}
        <code className="font-mono text-xs">/v1/chat/completions</code> to see
        model usage here.
      </p>
    );
  }

  const maxTokens = Math.max(...tokens_by_model.map((m) => m.tokens_total), 1);

  return (
    <div className="overflow-x-auto">
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>Model</TableHead>
            <TableHead>Requests</TableHead>
            <TableHead>Tokens in</TableHead>
            <TableHead>Tokens out</TableHead>
            <TableHead>Total tokens</TableHead>
            <TableHead>Est. spend</TableHead>
            <TableHead className="w-48">Share</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {tokens_by_model.map((m) => (
            <TableRow key={m.model}>
              <TableCell className="font-mono text-xs">{m.model}</TableCell>
              <TableCell className="font-mono text-xs">
                {m.requests.toLocaleString()}
              </TableCell>
              <TableCell className="font-mono text-xs">
                {formatTokens(m.tokens_in)}
              </TableCell>
              <TableCell className="font-mono text-xs">
                {formatTokens(m.tokens_out)}
              </TableCell>
              <TableCell className="font-mono text-xs">
                {formatTokens(m.tokens_total)}
              </TableCell>
              <TableCell className="font-mono text-xs">
                ${m.spend_usd.toFixed(4)}
              </TableCell>
              <TableCell>
                <div className="h-2 w-full bg-muted rounded-full overflow-hidden">
                  <div
                    className="h-full bg-primary rounded-full"
                    style={{
                      width: `${Math.round((m.tokens_total / maxTokens) * 100)}%`,
                    }}
                  />
                </div>
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </div>
  );
}

async function DailySpendChart() {
  const { daily_spend } = await fetchAnalytics();

  if (daily_spend.length === 0) {
    return (
      <p className="text-sm text-muted-foreground">No daily data yet.</p>
    );
  }

  const maxTokens = Math.max(...daily_spend.map((d) => d.tokens), 1);

  return (
    <div className="overflow-x-auto">
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>Date</TableHead>
            <TableHead>Requests</TableHead>
            <TableHead>Tokens</TableHead>
            <TableHead>Est. spend</TableHead>
            <TableHead className="w-48">Volume</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {daily_spend.map((d) => (
            <TableRow key={d.date}>
              <TableCell className="font-mono text-xs">{d.date}</TableCell>
              <TableCell className="font-mono text-xs">
                {d.requests.toLocaleString()}
              </TableCell>
              <TableCell className="font-mono text-xs">
                {formatTokens(d.tokens)}
              </TableCell>
              <TableCell className="font-mono text-xs">
                ${d.spend_usd.toFixed(4)}
              </TableCell>
              <TableCell>
                <div className="h-2 w-full bg-muted rounded-full overflow-hidden">
                  <div
                    className="h-full bg-primary rounded-full"
                    style={{
                      width: `${Math.round((d.tokens / maxTokens) * 100)}%`,
                    }}
                  />
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

// ---------- Formatters ----------

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(2)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

// ---------- Skeletons ----------

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
