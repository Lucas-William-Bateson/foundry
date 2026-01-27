import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { fetchRepos, type Repo } from "@/lib/api";
import { formatRelativeTime } from "@/lib/utils";
import {
  GitBranch,
  ExternalLink,
  Loader2,
  CheckCircle2,
  XCircle,
} from "lucide-react";

export function Repositories() {
  const [repos, setRepos] = useState<Repo[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const load = async () => {
      try {
        const data = await fetchRepos();
        setRepos(data);
      } catch (e) {
        console.error("Failed to load repos:", e);
      } finally {
        setLoading(false);
      }
    };
    load();
  }, []);

  if (loading) {
    return (
      <div className="flex items-center justify-center h-64">
        <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <h1 className="text-3xl font-bold">Repositories</h1>

      {repos.length === 0 ? (
        <Card>
          <CardContent className="py-12 text-center text-muted-foreground">
            No repositories yet. Push to a configured repo to get started!
          </CardContent>
        </Card>
      ) : (
        <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
          {repos.map((repo) => {
            const successRate =
              repo.build_count > 0
                ? ((repo.success_count / repo.build_count) * 100).toFixed(0)
                : null;

            return (
              <Link to={`/repo/${repo.id}`} key={repo.id}>
                <Card className="hover:border-primary/50 transition-colors cursor-pointer h-full">
                <CardHeader>
                  <div className="flex items-start justify-between">
                    <div>
                      <CardTitle className="flex items-center gap-2">
                        <GitBranch className="h-5 w-5" />
                        {repo.name}
                      </CardTitle>
                      <p className="text-sm text-muted-foreground mt-1">
                        {repo.owner}
                      </p>
                    </div>
                    {repo.last_status && (
                      <Badge
                        variant={
                          repo.last_status === "success"
                            ? "success"
                            : "destructive"
                        }
                      >
                        {repo.last_status === "success" ? (
                          <CheckCircle2 className="h-3 w-3 mr-1" />
                        ) : (
                          <XCircle className="h-3 w-3 mr-1" />
                        )}
                        {repo.last_status}
                      </Badge>
                    )}
                  </div>
                </CardHeader>
                <CardContent>
                  {repo.description && (
                    <p className="text-sm text-muted-foreground mb-4 line-clamp-2">
                      {repo.description}
                    </p>
                  )}

                  <div className="grid grid-cols-3 gap-4 text-center">
                    <div>
                      <div className="text-2xl font-bold">
                        {repo.build_count}
                      </div>
                      <div className="text-xs text-muted-foreground">
                        Builds
                      </div>
                    </div>
                    <div>
                      <div className="text-2xl font-bold text-green-500">
                        {repo.success_count}
                      </div>
                      <div className="text-xs text-muted-foreground">
                        Passed
                      </div>
                    </div>
                    <div>
                      <div className="text-2xl font-bold text-red-500">
                        {repo.failure_count}
                      </div>
                      <div className="text-xs text-muted-foreground">
                        Failed
                      </div>
                    </div>
                  </div>

                  {(successRate || repo.last_build_at) && (
                    <div className="mt-4 pt-4 border-t text-sm text-muted-foreground">
                      {successRate && <span>{successRate}% success rate</span>}
                      {repo.last_build_at && (
                        <span className="float-right">
                          Last build {formatRelativeTime(repo.last_build_at)}
                        </span>
                      )}
                    </div>
                  )}

                  {repo.html_url && (
                    <a
                      href={repo.html_url}
                      target="_blank"
                      rel="noopener noreferrer"
                      onClick={(e) => e.stopPropagation()}
                      className="mt-4 flex items-center justify-center gap-2 text-sm text-primary hover:underline"
                    >
                      View on GitHub
                      <ExternalLink className="h-3 w-3" />
                    </a>
                  )}
                </CardContent>
              </Card>
              </Link>
            );
          })}
        </div>
      )}
    </div>
  );
}
