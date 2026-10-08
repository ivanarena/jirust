package main

import (
	"context"
	"flag"
	"fmt"
	"os"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/ivanarena/jiraui/internal/cache"
	"github.com/ivanarena/jiraui/internal/config"
	"github.com/ivanarena/jiraui/internal/jira"
	"github.com/ivanarena/jiraui/internal/ui"
)

func main() {
	backend := flag.String("backend", "auto", "auto, acli, jira, or demo")
	demo := flag.Bool("demo", false, "use synthetic demo data; never access Jira")
	binary := flag.String("cli-bin", "", "override the selected CLI executable")
	readOnly := flag.Bool("read-only", false, "disable all ticket changes")
	noCache := flag.Bool("no-cache", false, "do not cache ticket content on disk")
	project := flag.String("project", "", "initial project filter")
	jql := flag.String("jql", "", "initial JQL (overrides structured filters)")
	limit := flag.Int("limit", 100, "maximum tickets per query, 1–100")
	version := flag.Bool("version", false, "print version")
	check := flag.Bool("check", false, "read-only search/detail connectivity check; no terminal UI")
	flag.Parse()
	if *version {
		fmt.Println("jiraui 0.1.0")
		return
	}
	if *limit < 1 || *limit > 100 {
		fmt.Fprintln(os.Stderr, "--limit must be between 1 and 100")
		os.Exit(2)
	}
	cfg, err := config.Load()
	if err != nil {
		fmt.Fprintln(os.Stderr, "Cannot load preferences:", err)
		os.Exit(1)
	}
	if *project != "" {
		cfg.Filters.Project = *project
	}
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	var client jira.Backend
	var scope string
	if *demo || *backend == "demo" {
		client = jira.NewDemo()
		cfg = config.Config{}
		if *project != "" {
			cfg.Filters.Project = *project
		}
		scope = "demo"
		*noCache = true
	} else {
		c, err := jira.Resolve(*backend, *binary)
		if err != nil {
			fmt.Fprintln(os.Stderr, err)
			os.Exit(1)
		}
		c.Runner = jira.ProcessRunner{Context: ctx}
		client = c
		scope = c.Scope()
		if scope == "" {
			*noCache = true
		}
	}
	if *check {
		query := jira.Query("mine", cfg.Filters, "", *jql)
		items, err := client.Search(query, *limit)
		if err != nil {
			fmt.Fprintln(os.Stderr, "Read-only search failed:", err)
			os.Exit(1)
		}
		if len(items) > 0 {
			if _, err = client.Get(items[0].Key); err != nil {
				fmt.Fprintln(os.Stderr, "Read-only detail failed:", err)
				os.Exit(1)
			}
		}
		fmt.Printf("backend=%s search=ok tickets=%d detail=%s (no records changed)\n", client.Name(), len(items), map[bool]string{true: "ok", false: "skipped: empty query"}[len(items) > 0])
		return
	}
	store, err := cache.New(scope, *noCache)
	if err != nil {
		fmt.Fprintln(os.Stderr, "Cache unavailable:", err)
		os.Exit(1)
	}
	model := ui.New(client, store, cfg, *readOnly, *limit, *jql)
	// Mouse clicks go to this TUI in a Herdr pane; URLs are explicitly opened.
	p := tea.NewProgram(model, tea.WithAltScreen(), tea.WithMouseCellMotion())
	_, err = p.Run()
	cancel()
	if err != nil {
		fmt.Fprintln(os.Stderr, "Terminal error:", err)
		os.Exit(1)
	}
}
