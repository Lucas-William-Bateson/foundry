import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import Grid from "@mui/material/Grid";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { fetchRepos, type Repo } from "@/lib/api";
import { formatRelativeTime } from "@/lib/utils";
import AccountTree from "@mui/icons-material/AccountTree";
import OpenInNew from "@mui/icons-material/OpenInNew";
import Autorenew from "@mui/icons-material/Autorenew";
import CheckCircle from "@mui/icons-material/CheckCircle";
import ErrorIcon from "@mui/icons-material/Error";

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
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          height: "16rem",
        }}
      >
        <Autorenew
          className="spin-animation"
          style={{ fontSize: 32, color: "#8B8F96" }}
        />
      </div>
    );
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "1.5rem" }}>
      <h1
        style={{
          fontSize: "1.75rem",
          fontWeight: 700,
          letterSpacing: "-0.02em",
        }}
      >
        Repositories
      </h1>

      {repos.length === 0 ? (
        <Card>
          <CardContent
            style={{ padding: "3rem", textAlign: "center", color: "#8B8F96" }}
          >
            No repositories yet. Push to a configured repo to get started!
          </CardContent>
        </Card>
      ) : (
        <Grid container spacing={2}>
          {repos.map((repo) => {
            const successRate =
              repo.build_count > 0
                ? ((repo.success_count / repo.build_count) * 100).toFixed(0)
                : null;

            return (
              <Grid size={{ xs: 12, sm: 6, md: 4 }} key={repo.id}>
                <Link
                  to={`/repo/${repo.id}`}
                  style={{
                    textDecoration: "none",
                    color: "inherit",
                    display: "block",
                    marginBottom: "1rem",
                  }}
                >
                  <Card
                    style={{
                      height: "100%",
                      cursor: "pointer",
                      transition: "border-color 0.15s",
                    }}
                  >
                    <CardHeader>
                      <div
                        style={{
                          display: "flex",
                          alignItems: "flex-start",
                          justifyContent: "space-between",
                        }}
                      >
                        <div>
                          <CardTitle
                            style={{
                              display: "flex",
                              alignItems: "center",
                              gap: "0.5rem",
                            }}
                          >
                            <AccountTree style={{ fontSize: 20 }} />
                            {repo.name}
                          </CardTitle>
                          <p
                            style={{
                              fontSize: "0.875rem",
                              color: "#8B8F96",
                              marginTop: "0.25rem",
                            }}
                          >
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
                              <CheckCircle
                                style={{ fontSize: 12, marginRight: "0.25rem" }}
                              />
                            ) : (
                              <ErrorIcon
                                style={{ fontSize: 12, marginRight: "0.25rem" }}
                              />
                            )}
                            {repo.last_status}
                          </Badge>
                        )}
                      </div>
                    </CardHeader>
                    <CardContent>
                      {repo.description && (
                        <p
                          style={{
                            fontSize: "0.875rem",
                            color: "#8B8F96",
                            marginBottom: "1rem",
                            overflow: "hidden",
                            display: "-webkit-box",
                            WebkitLineClamp: 2,
                            WebkitBoxOrient: "vertical",
                          }}
                        >
                          {repo.description}
                        </p>
                      )}

                      <div
                        style={{
                          display: "grid",
                          gridTemplateColumns: "1fr 1fr 1fr",
                          gap: "1rem",
                          textAlign: "center",
                        }}
                      >
                        <div>
                          <div
                            style={{
                              fontSize: "1.75rem",
                              fontWeight: 700,
                              letterSpacing: "-0.02em",
                              fontVariantNumeric: "tabular-nums",
                            }}
                          >
                            {repo.build_count}
                          </div>
                          <div
                            style={{
                              fontSize: "0.6875rem",
                              color: "#8B8F96",
                              letterSpacing: "0.04em",
                              textTransform: "uppercase",
                            }}
                          >
                            Builds
                          </div>
                        </div>
                        <div>
                          <div
                            style={{
                              fontSize: "1.75rem",
                              fontWeight: 700,
                              letterSpacing: "-0.02em",
                              fontVariantNumeric: "tabular-nums",
                              color: "#2D9D5E",
                            }}
                          >
                            {repo.success_count}
                          </div>
                          <div
                            style={{
                              fontSize: "0.6875rem",
                              color: "#8B8F96",
                              letterSpacing: "0.04em",
                              textTransform: "uppercase",
                            }}
                          >
                            Passed
                          </div>
                        </div>
                        <div>
                          <div
                            style={{
                              fontSize: "1.75rem",
                              fontWeight: 700,
                              letterSpacing: "-0.02em",
                              fontVariantNumeric: "tabular-nums",
                              color: "#D44B4B",
                            }}
                          >
                            {repo.failure_count}
                          </div>
                          <div
                            style={{
                              fontSize: "0.6875rem",
                              color: "#8B8F96",
                              letterSpacing: "0.04em",
                              textTransform: "uppercase",
                            }}
                          >
                            Failed
                          </div>
                        </div>
                      </div>

                      {(successRate || repo.last_build_at) && (
                        <div
                          style={{
                            marginTop: "1rem",
                            fontSize: "0.8125rem",
                            color: "#8B8F96",
                            display: "flex",
                            justifyContent: "space-between",
                          }}
                        >
                          {successRate && (
                            <span>{successRate}% success rate</span>
                          )}
                          {repo.last_build_at && (
                            <span>
                              Last build{" "}
                              {formatRelativeTime(repo.last_build_at)}
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
                          style={{
                            marginTop: "1rem",
                            display: "flex",
                            alignItems: "center",
                            justifyContent: "center",
                            gap: "0.5rem",
                            fontSize: "0.8125rem",
                            color: "#C65D00",
                          }}
                        >
                          View on GitHub
                          <OpenInNew style={{ fontSize: 12 }} />
                        </a>
                      )}
                    </CardContent>
                    {/* Success rate bar */}
                    {successRate && (
                      <div
                        style={{
                          height: "2px",
                          borderRadius: "0 0 6px 6px",
                          overflow: "hidden",
                          background: "rgba(255,255,255,0.03)",
                        }}
                      >
                        <div
                          style={{
                            height: "100%",
                            width: `${successRate}%`,
                            background: "#2D9D5E40",
                            borderRadius: "0 0 0 6px",
                          }}
                        />
                      </div>
                    )}
                  </Card>
                </Link>
              </Grid>
            );
          })}
        </Grid>
      )}
    </div>
  );
}
