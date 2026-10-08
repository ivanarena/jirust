package jira

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"time"
)

type Runner interface {
	Run(string, ...string) ([]byte, error)
}
type ProcessRunner struct {
	Timeout time.Duration
	Context context.Context
}

// Bound CLI processes even if users navigate faster than network responses.
var processSlots = make(chan struct{}, 3)

func (r ProcessRunner) Run(binary string, args ...string) ([]byte, error) {
	timeout := r.Timeout
	if timeout == 0 {
		timeout = 40 * time.Second
	}
	parent := r.Context
	if parent == nil {
		parent = context.Background()
	}
	ctx, cancel := context.WithTimeout(parent, timeout)
	defer cancel()
	select {
	case processSlots <- struct{}{}:
		defer func() { <-processSlots }()
	case <-ctx.Done():
		return nil, fmt.Errorf("%s request cancelled or timed out while waiting for a CLI slot", binary)
	}
	cmd := exec.CommandContext(ctx, binary, args...)
	cmd.Env = append(os.Environ(), "NO_COLOR=1", "CLICOLOR=0")
	// No shell, no debug logging, no interactive stdin. Credentials stay in the CLI.
	output, err := cmd.CombinedOutput()
	if ctx.Err() != nil {
		return nil, fmt.Errorf("%s timed out; reload to check the server before retrying a change", binary)
	}
	if err != nil {
		msg := strings.TrimSpace(Safe(string(output)))
		if len(msg) > 1800 {
			msg = msg[:1800]
		}
		if msg == "" {
			msg = err.Error()
		}
		return nil, fmt.Errorf("%s: %s", binary, msg)
	}
	return output, nil
}

type CLI struct {
	Kind   string
	Binary string
	Runner Runner
}

func (c *CLI) Name() string                       { return c.Kind }
func (c *CLI) run(args ...string) ([]byte, error) { return c.Runner.Run(c.Binary, args...) }
func list[T any](data []byte, fields ...string) ([]T, error) {
	var items []T
	if json.Unmarshal(data, &items) == nil {
		return items, nil
	}
	var envelope map[string]json.RawMessage
	if err := json.Unmarshal(data, &envelope); err != nil {
		return nil, fmt.Errorf("CLI returned invalid JSON: %w", err)
	}
	for _, key := range fields {
		if raw, ok := envelope[key]; ok {
			if err := json.Unmarshal(raw, &items); err != nil {
				return nil, err
			}
			return items, nil
		}
	}
	return nil, errors.New("CLI JSON has no recognized collection; update the CLI or report the response shape")
}
func (c *CLI) Search(query string, limit int) ([]Issue, error) {
	var b []byte
	var err error
	if c.Kind == "jira" {
		if limit > 100 {
			limit = 100
		}
		b, err = c.run("issue", "list", "--jql", query, "--raw", "--paginate", strconv.Itoa(limit))
	} else {
		b, err = c.run("jira", "workitem", "search", "--jql", query, "--fields", "key,summary,status,assignee,reporter,priority,issuetype,labels", "--limit", strconv.Itoa(limit), "--json")
	}
	if err != nil {
		return nil, err
	}
	return list[Issue](b, "issues", "values")
}
func checkKey(key string) error {
	if !IssueKey.MatchString(key) {
		return fmt.Errorf("invalid ticket key %q", SafeLine(key))
	}
	return nil
}
func (c *CLI) Get(key string) (Issue, error) {
	var i Issue
	if err := checkKey(key); err != nil {
		return i, err
	}
	var b []byte
	var err error
	if c.Kind == "jira" {
		b, err = c.run("issue", "view", key, "--raw")
	} else {
		b, err = c.run("jira", "workitem", "view", key, "--fields", "*all", "--json")
	}
	if err != nil {
		return i, err
	}
	err = json.Unmarshal(b, &i)
	return i, err
}
func (c *CLI) Comments(key string) ([]Comment, error) {
	if err := checkKey(key); err != nil {
		return nil, err
	}
	if c.Kind == "jira" {
		i, err := c.Get(key)
		if err != nil {
			return nil, err
		}
		if i.Fields.Comments.Total > len(i.Fields.Comments.Comments) {
			return nil, errors.New("jira-cli does not expose comment pagination; use the acli backend to load all comments")
		}
		return i.Fields.Comments.Comments, nil
	}
	b, err := c.run("jira", "workitem", "comment", "list", "--key", key, "--paginate", "--json")
	if err != nil {
		return nil, err
	}
	return list[Comment](b, "comments", "values")
}
func (c *CLI) Boards(project string) ([]Board, error) {
	if c.Kind == "jira" {
		return nil, errors.New("board discovery requires the acli backend; Jira CLI board list does not expose JSON")
	}
	args := []string{"jira", "board", "search", "--json", "--limit", "100"}
	if project != "" {
		args = append(args, "--project", project)
	}
	b, err := c.run(args...)
	if err != nil {
		return nil, err
	}
	return list[Board](b, "values", "boards")
}
func (c *CLI) Sprints(board int) ([]Sprint, error) {
	if c.Kind == "jira" {
		return nil, errors.New("sprint discovery requires the acli backend")
	}
	b, err := c.run("jira", "board", "list-sprints", "--id", strconv.Itoa(board), "--state", "active,future", "--limit", "100", "--json")
	if err != nil {
		return nil, err
	}
	return list[Sprint](b, "values", "sprints")
}
func (c *CLI) SprintIssues(board, sprint int, query string, limit int) ([]Issue, error) {
	if c.Kind == "jira" {
		b, err := c.run("sprint", "list", strconv.Itoa(sprint), "--jql", query, "--raw", "--paginate", strconv.Itoa(min(limit, 100)))
		if err != nil {
			return nil, err
		}
		return list[Issue](b, "issues", "values")
	}
	b, err := c.run("jira", "sprint", "list-workitems", "--board", strconv.Itoa(board), "--sprint", strconv.Itoa(sprint), "--jql", query, "--limit", strconv.Itoa(limit), "--json")
	if err != nil {
		return nil, err
	}
	return list[Issue](b, "issues", "values")
}
func bodyFile(text string) (string, func(), error) {
	f, err := os.CreateTemp("", "jiraui-body-*")
	if err != nil {
		return "", nil, err
	}
	remove := func() { _ = os.Remove(f.Name()) }
	if _, err = f.WriteString(text); err != nil {
		f.Close()
		remove()
		return "", nil, err
	}
	if err = f.Close(); err != nil {
		remove()
		return "", nil, err
	}
	return f.Name(), remove, nil
}
func (c *CLI) Apply(a Action) error {
	if err := checkKey(a.Key); err != nil {
		return err
	}
	if strings.TrimSpace(a.Value) == "" {
		return errors.New("empty update rejected")
	}
	var args []string
	if c.Kind == "jira" {
		switch a.Kind {
		case "summary", "description", "labels", "type":
			flags := map[string]string{"summary": "--summary", "description": "--body", "labels": "--label"}
			flag, ok := flags[a.Kind]
			if !ok {
				return errors.New("jira-cli does not support changing issue type with this adapter")
			}
			args = []string{"issue", "edit", a.Key, flag, a.Value, "--no-input"}
		case "assign":
			value := a.Value
			if value == "@me" {
				b, err := c.run("me")
				if err != nil {
					return err
				}
				value = strings.TrimSpace(string(b))
			}
			if value == "unassigned" {
				value = "x"
			}
			args = []string{"issue", "assign", a.Key, value}
		case "status":
			args = []string{"issue", "move", a.Key, a.Value}
		case "comment":
			file, remove, err := bodyFile(a.Value)
			if err != nil {
				return err
			}
			defer remove()
			args = []string{"issue", "comment", "add", a.Key, "--template", file, "--no-input"}
		case "link":
			if err := checkKey(a.Value); err != nil {
				return err
			}
			if strings.TrimSpace(a.Extra) == "" {
				return errors.New("link type is required")
			}
			args = []string{"issue", "link", a.Key, a.Value, a.Extra}
		case "sprint":
			n, err := strconv.Atoi(a.Value)
			if err != nil || n < 1 {
				return errors.New("sprint ID must be a positive integer")
			}
			args = []string{"sprint", "add", a.Value, a.Key}
		default:
			return fmt.Errorf("unsupported Jira CLI action: %s", a.Kind)
		}
	} else {
		switch a.Kind {
		case "summary", "labels", "type":
			args = []string{"jira", "workitem", "edit", "--key", a.Key, "--" + a.Kind, a.Value, "--yes", "--json"}
		case "description":
			file, remove, err := bodyFile(a.Value)
			if err != nil {
				return err
			}
			defer remove()
			args = []string{"jira", "workitem", "edit", "--key", a.Key, "--description-file", file, "--yes", "--json"}
		case "assign":
			args = []string{"jira", "workitem", "assign", "--key", a.Key, "--yes", "--json"}
			if a.Value == "unassigned" {
				args = append(args, "--remove-assignee")
			} else {
				args = append(args, "--assignee", a.Value)
			}
		case "status":
			args = []string{"jira", "workitem", "transition", "--key", a.Key, "--status", a.Value, "--yes", "--json"}
		case "comment":
			file, remove, err := bodyFile(a.Value)
			if err != nil {
				return err
			}
			defer remove()
			args = []string{"jira", "workitem", "comment", "create", "--key", a.Key, "--body-file", file, "--json"}
		case "link":
			if err := checkKey(a.Value); err != nil {
				return err
			}
			if strings.TrimSpace(a.Extra) == "" {
				return errors.New("link type is required")
			}
			args = []string{"jira", "workitem", "link", "create", "--out", a.Key, "--in", a.Value, "--type", a.Extra, "--yes"}
		case "sprint":
			return errors.New("acli has no move-ticket-to-sprint action; use --backend jira with its own configured credentials")
		default:
			return fmt.Errorf("unsupported Atlassian CLI action: %s", a.Kind)
		}
	}
	output, err := c.run(args...)
	if err != nil {
		return err
	}
	return receiptError(output)
}

// Some CLI bulk operations report per-item failures in otherwise valid JSON.
func receiptError(output []byte) error {
	var value any
	if json.Unmarshal(output, &value) != nil {
		return nil
	}
	var failed func(any) bool
	failed = func(v any) bool {
		switch x := v.(type) {
		case []any:
			for _, item := range x {
				if failed(item) {
					return true
				}
			}
		case map[string]any:
			for key, item := range x {
				switch strings.ToLower(key) {
				case "success":
					if b, ok := item.(bool); ok && !b {
						return true
					}
				case "errors", "errormessages", "failures", "failed":
					switch e := item.(type) {
					case string:
						if e != "" {
							return true
						}
					case []any:
						if len(e) > 0 {
							return true
						}
					case map[string]any:
						if len(e) > 0 {
							return true
						}
					case float64:
						if e > 0 {
							return true
						}
					case bool:
						if e {
							return true
						}
					}
				}
				if failed(item) {
					return true
				}
			}
		}
		return false
	}
	if failed(value) {
		return fmt.Errorf("CLI reported an unsuccessful update: %s", SafeLine(string(output)))
	}
	return nil
}
func Resolve(kind, binary string) (*CLI, error) {
	if kind == "auto" {
		if _, err := exec.LookPath("acli"); err == nil {
			kind = "acli"
		} else {
			kind = "jira"
		}
	}
	if kind != "acli" && kind != "jira" {
		return nil, fmt.Errorf("unknown backend %q", kind)
	}
	if binary == "" {
		binary = kind
		if kind == "jira" {
			if _, err := os.Stat("/usr/bin/jira"); err == nil {
				binary = "/usr/bin/jira"
			}
		}
	}
	path, err := exec.LookPath(binary)
	if err != nil {
		return nil, fmt.Errorf("%s is not installed: %w", binary, err)
	}
	return &CLI{Kind: kind, Binary: path, Runner: ProcessRunner{}}, nil
}

// Scope separates cached data by CLI account/config without saving login details.
func (c *CLI) Scope() string {
	if c.Kind == "acli" {
		if b, err := c.run("jira", "auth", "status"); err == nil {
			return c.Binary + ":" + string(b)
		}
		return "" // No verified account: never reuse an unscoped on-disk cache.
	}
	path := os.Getenv("JIRA_CONFIG_FILE")
	if path == "" {
		if root, err := os.UserConfigDir(); err == nil {
			path = filepath.Join(root, ".jira", ".config.yml")
		}
	}
	cfg, _ := os.ReadFile(path)
	// Include the existing config/token only as in-memory input to the cache's
	// SHA-256 namespace. Never log or persist a raw scope.
	return c.Binary + ":" + path + ":" + string(cfg) + ":" + os.Getenv("JIRA_API_TOKEN") + ":" + os.Getenv("JIRA_AUTH_TYPE")
}
