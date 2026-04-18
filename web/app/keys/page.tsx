"use client";

import { useEffect, useState, useTransition } from "react";
import Link from "next/link";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  CardDescription,
} from "@/components/ui/card";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Separator } from "@/components/ui/separator";
import { Skeleton } from "@/components/ui/skeleton";

// ---------- Types ----------

interface ApiKey {
  id: string;
  owner: string;
  rate_limit_rpm: number;
  created_at: string;
  revoked_at: string | null;
}

interface CreatedKey extends ApiKey {
  key: string;
}

interface KeyUsageSummary {
  key_id: string;
  request_count: number;
  total_tokens_in: number;
  total_tokens_out: number;
  total_tokens: number;
}

// ---------- Page ----------

export default function KeysPage() {
  const [keys, setKeys] = useState<ApiKey[]>([]);
  const [usageMap, setUsageMap] = useState<Map<string, KeyUsageSummary>>(
    new Map()
  );
  const [loading, setLoading] = useState(true);
  const [newKey, setNewKey] = useState<CreatedKey | null>(null);
  const [owner, setOwner] = useState("");
  const [rpm, setRpm] = useState("60");
  const [creating, startCreate] = useTransition();
  const [revoking, setRevoking] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  async function loadData() {
    setLoading(true);
    try {
      const [keysRes, usageRes] = await Promise.all([
        fetch("/api/keys"),
        fetch("/api/usage"),
      ]);
      const keysBody = await keysRes.json();
      const usageBody = await usageRes.json();

      setKeys(keysBody.data ?? []);

      const map = new Map<string, KeyUsageSummary>();
      for (const entry of usageBody.by_key ?? []) {
        map.set(entry.key_id, entry);
      }
      setUsageMap(map);
    } catch {
      setError("Could not reach the gateway. Is it running?");
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    loadData();
  }, []);

  function handleCreate() {
    if (!owner.trim()) return;
    setError(null);

    startCreate(async () => {
      try {
        const res = await fetch("/api/keys", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({
            owner: owner.trim(),
            rate_limit_rpm: parseInt(rpm, 10) || 60,
          }),
        });

        if (!res.ok) {
          const body = await res.json();
          setError(body?.error?.message ?? "Failed to create key");
          return;
        }

        const created: CreatedKey = await res.json();
        setNewKey(created);
        setOwner("");
        setRpm("60");
        await loadData();
      } catch {
        setError("Failed to create key — gateway unreachable");
      }
    });
  }

  async function handleRevoke(id: string) {
    if (!confirm("Revoke this key? This cannot be undone.")) return;
    setRevoking(id);
    setError(null);

    try {
      const res = await fetch(`/api/keys/${id}`, { method: "DELETE" });
      if (!res.ok && res.status !== 204) {
        setError("Failed to revoke key");
      } else {
        await loadData();
      }
    } catch {
      setError("Failed to revoke key — gateway unreachable");
    } finally {
      setRevoking(null);
    }
  }

  async function copyKey(key: string) {
    await navigator.clipboard.writeText(key);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  }

  const activeKeys = keys.filter((k) => !k.revoked_at);
  const revokedKeys = keys.filter((k) => k.revoked_at);

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
            className="text-foreground font-medium text-xs font-mono"
          >
            API Keys
          </Link>
          <Link
            href="/models"
            className="hover:text-foreground transition-colors text-xs"
          >
            Models
          </Link>
          <Link
            href="/dashboard/consumer"
            className="hover:text-foreground transition-colors text-xs"
          >
            Usage
          </Link>
          <Link
            href="/provider"
            className="hover:text-foreground transition-colors text-xs"
          >
            Provider
          </Link>
        </div>
      </nav>

      <div className="flex-1 px-6 py-10 max-w-5xl mx-auto w-full space-y-8">
        {/* New key banner */}
        {newKey && (
          <Card className="border-primary/40 bg-primary/5">
            <CardHeader>
              <CardTitle className="text-sm">Key created — save it now</CardTitle>
              <CardDescription>
                This is the only time this key will be shown.
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-3">
              <div className="flex items-center gap-3">
                <code className="flex-1 rounded-md bg-muted px-3 py-2 font-mono text-xs break-all">
                  {newKey.key}
                </code>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => copyKey(newKey.key)}
                >
                  {copied ? "Copied!" : "Copy"}
                </Button>
              </div>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => setNewKey(null)}
              >
                Dismiss
              </Button>
            </CardContent>
          </Card>
        )}

        {/* Error banner */}
        {error && (
          <div className="rounded-lg bg-destructive/10 border border-destructive/30 px-4 py-3 text-sm text-destructive">
            {error}
          </div>
        )}

        {/* Create form */}
        <Card>
          <CardHeader>
            <CardTitle>Create API key</CardTitle>
            <CardDescription>
              Keys authenticate requests to{" "}
              <code className="font-mono text-xs">/v1/chat/completions</code>{" "}
              and <code className="font-mono text-xs">/v1/models</code>.
            </CardDescription>
          </CardHeader>
          <CardContent>
            <div className="flex flex-col sm:flex-row gap-3">
              <input
                type="text"
                placeholder="Owner name (e.g. alice)"
                value={owner}
                onChange={(e) => setOwner(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && handleCreate()}
                className="flex-1 h-8 rounded-lg border border-input bg-transparent px-3 text-sm outline-none focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/30 placeholder:text-muted-foreground"
              />
              <input
                type="number"
                placeholder="RPM"
                value={rpm}
                onChange={(e) => setRpm(e.target.value)}
                className="w-24 h-8 rounded-lg border border-input bg-transparent px-3 text-sm outline-none focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/30 placeholder:text-muted-foreground"
                min={1}
                max={10000}
              />
              <Button
                onClick={handleCreate}
                disabled={creating || !owner.trim()}
                size="default"
              >
                {creating ? "Creating…" : "Create key"}
              </Button>
            </div>
          </CardContent>
        </Card>

        <Separator />

        {/* Active keys table */}
        <section>
          <h2 className="text-base font-semibold mb-4">
            Active keys{" "}
            {!loading && (
              <span className="text-muted-foreground font-normal text-sm">
                ({activeKeys.length})
              </span>
            )}
          </h2>

          {loading ? (
            <div className="space-y-2">
              {[0, 1, 2].map((i) => (
                <Skeleton key={i} className="h-10 w-full" />
              ))}
            </div>
          ) : activeKeys.length === 0 ? (
            <p className="text-sm text-muted-foreground">
              No active keys. Create one above.
            </p>
          ) : (
            <div className="overflow-x-auto">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>ID</TableHead>
                    <TableHead>Owner</TableHead>
                    <TableHead>Rate limit</TableHead>
                    <TableHead>Requests</TableHead>
                    <TableHead>Tokens</TableHead>
                    <TableHead>Created</TableHead>
                    <TableHead />
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {activeKeys.map((key) => {
                    const usage = usageMap.get(key.id);
                    return (
                      <TableRow key={key.id}>
                        <TableCell className="font-mono text-xs">
                          {key.id.slice(0, 8)}…
                        </TableCell>
                        <TableCell className="text-xs">{key.owner}</TableCell>
                        <TableCell className="font-mono text-xs">
                          {key.rate_limit_rpm} rpm
                        </TableCell>
                        <TableCell className="font-mono text-xs">
                          {usage ? usage.request_count.toLocaleString() : (
                            <span className="text-muted-foreground">—</span>
                          )}
                        </TableCell>
                        <TableCell className="font-mono text-xs">
                          {usage ? formatTokens(usage.total_tokens) : (
                            <span className="text-muted-foreground">—</span>
                          )}
                        </TableCell>
                        <TableCell className="text-xs text-muted-foreground">
                          {new Date(key.created_at).toLocaleDateString()}
                        </TableCell>
                        <TableCell className="text-right">
                          <Button
                            variant="destructive"
                            size="xs"
                            onClick={() => handleRevoke(key.id)}
                            disabled={revoking === key.id}
                          >
                            {revoking === key.id ? "Revoking…" : "Revoke"}
                          </Button>
                        </TableCell>
                      </TableRow>
                    );
                  })}
                </TableBody>
              </Table>
            </div>
          )}
        </section>

        {/* Revoked keys (collapsed) */}
        {!loading && revokedKeys.length > 0 && (
          <>
            <Separator />
            <section>
              <h2 className="text-base font-semibold mb-4 text-muted-foreground">
                Revoked keys ({revokedKeys.length})
              </h2>
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>ID</TableHead>
                    <TableHead>Owner</TableHead>
                    <TableHead>Revoked</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {revokedKeys.map((key) => (
                    <TableRow key={key.id} className="opacity-50">
                      <TableCell className="font-mono text-xs">
                        {key.id.slice(0, 8)}…
                      </TableCell>
                      <TableCell className="text-xs">{key.owner}</TableCell>
                      <TableCell className="text-xs text-muted-foreground">
                        {key.revoked_at
                          ? new Date(key.revoked_at).toLocaleDateString()
                          : "—"}
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </section>
          </>
        )}
      </div>
    </main>
  );
}

// ---------- Formatter ----------

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(2)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}
