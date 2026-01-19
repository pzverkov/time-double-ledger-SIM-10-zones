package util

import (
  "runtime"
  "runtime/debug"
  "strings"
  "time"
)

type VersionInfo struct {
  Service    string `json:"service"`
  Language   string `json:"language"`
  Version    string `json:"version"`
  Revision   string `json:"revision"`
  BuildTime  string `json:"build_time"`
  Modified   bool   `json:"modified"`
  GoVersion  string `json:"go_version"`
  Platform   string `json:"platform"`
}

func GetVersionInfo(service string) VersionInfo {
  vi := VersionInfo{
    Service:   service,
    Language: "go",
    GoVersion: runtime.Version(),
    Platform: runtime.GOOS + "/" + runtime.GOARCH,
  }

  if bi, ok := debug.ReadBuildInfo(); ok {
    vi.Version = bi.Main.Version
    // Extract VCS settings if present
    for _, s := range bi.Settings {
      switch s.Key {
      case "vcs.revision":
        vi.Revision = s.Value
      case "vcs.time":
        // normalize
        if t, err := time.Parse(time.RFC3339Nano, s.Value); err == nil {
          vi.BuildTime = t.UTC().Format(time.RFC3339Nano)
        } else {
          vi.BuildTime = s.Value
        }
      case "vcs.modified":
        vi.Modified = (s.Value == "true")
      }
    }
  }

  if vi.Version == "" {
    vi.Version = "(devel)"
  }
  // Trim very long revisions (just in case)
  if len(vi.Revision) > 12 {
    vi.Revision = strings.TrimSpace(vi.Revision[:12])
  }
  return vi
}
