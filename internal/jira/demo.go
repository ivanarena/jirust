package jira

import (
	"encoding/json"
	"fmt"
	"strings"
	"sync"
)

type Demo struct {
	mu    sync.Mutex
	items []Issue
}

func NewDemo() *Demo {
	raw := `[
 {"key":"DEV-101","self":"https://example.atlassian.net/rest/api/3/issue/101","fields":{"summary":"Ship the terminal workspace","status":{"name":"In Progress","statusCategory":{"key":"indeterminate"}},"assignee":{"displayName":"Alex","accountId":"alex"},"reporter":{"displayName":"Sam"},"priority":{"name":"High"},"issuetype":{"name":"Story"},"project":{"key":"DEV","name":"Demo"},"labels":["tui"],"description":{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"A fast Jira workspace. Follow DEV-102 internally or read "},{"type":"text","text":"Bubble Tea docs","marks":[{"type":"link","attrs":{"href":"https://github.com/charmbracelet/bubbletea"}}]}]}]},"issuelinks":[{"type":{"outward":"blocks","inward":"is blocked by"},"outwardIssue":{"key":"DEV-102","fields":{"summary":"Add filter presets"}}}],"comment":{"total":1,"comments":[{"id":"1","author":{"displayName":"Sam"},"created":"2026-01-01T10:00:00Z","body":"Please keep navigation instant."}]}}},
 {"key":"DEV-102","self":"https://example.atlassian.net/rest/api/3/issue/102","fields":{"summary":"Add saved searches and assignee filters","status":{"name":"To Do","statusCategory":{"key":"new"}},"assignee":{"displayName":"Sam","accountId":"sam"},"reporter":{"displayName":"Alex"},"priority":{"name":"Medium"},"issuetype":{"name":"Task"},"project":{"key":"DEV"},"labels":["filters"],"description":"Presets should combine project, assignee and status. Linked to DEV-101."}},
 {"key":"DEV-103","self":"https://example.atlassian.net/rest/api/3/issue/103","fields":{"summary":"Handle slow networks without blocking keys","status":{"name":"Review","statusCategory":{"key":"indeterminate"}},"assignee":{"displayName":"Alex"},"reporter":{"displayName":"Sam"},"priority":{"name":"High"},"issuetype":{"name":"Bug"},"project":{"key":"DEV"},"description":"Load results asynchronously. Never silently claim a write succeeded."}},
 {"key":"DEV-104","self":"https://example.atlassian.net/rest/api/3/issue/104","fields":{"summary":"Create a small release binary","status":{"name":"Done","statusCategory":{"key":"done"}},"assignee":null,"reporter":{"displayName":"Alex"},"priority":{"name":"Low"},"issuetype":{"name":"Task"},"project":{"key":"DEV"},"description":"No browser engine and no API credentials in the repository."}},
 {"key":"DEV-105","self":"https://example.atlassian.net/rest/api/3/issue/105","fields":{"summary":"Make links keyboard- and mouse-accessible","status":{"name":"To Do","statusCategory":{"key":"new"}},"assignee":null,"reporter":{"displayName":"Sam"},"priority":{"name":"Medium"},"issuetype":{"name":"Story"},"project":{"key":"DEV"},"description":"Open related tickets internally; ordinary HTTPS links externally."}}
 ]`
	var items []Issue
	_ = json.Unmarshal([]byte(raw), &items)
	return &Demo{items: items}
}
func (d *Demo) Name() string { return "demo" }
func clone[T any](v T) T     { b, _ := json.Marshal(v); var out T; _ = json.Unmarshal(b, &out); return out }
func (d *Demo) Search(_ string, limit int) ([]Issue, error) {
	d.mu.Lock()
	defer d.mu.Unlock()
	return clone(d.items[:min(len(d.items), limit)]), nil
}
func (d *Demo) Get(key string) (Issue, error) {
	d.mu.Lock()
	defer d.mu.Unlock()
	for _, i := range d.items {
		if i.Key == key {
			return clone(i), nil
		}
	}
	return Issue{}, fmt.Errorf("demo ticket %s not found", key)
}
func (d *Demo) Comments(key string) ([]Comment, error) {
	i, err := d.Get(key)
	return i.Fields.Comments.Comments, err
}
func (d *Demo) Boards(string) ([]Board, error) {
	return []Board{{ID: 1, Name: "Demo board", Type: "scrum"}}, nil
}
func (d *Demo) Sprints(int) ([]Sprint, error) {
	return []Sprint{{ID: 1, Name: "Demo sprint", State: "active"}}, nil
}
func (d *Demo) SprintIssues(_ int, _ int, q string, n int) ([]Issue, error) { return d.Search(q, n) }
func (d *Demo) Apply(a Action) error {
	d.mu.Lock()
	defer d.mu.Unlock()
	for n := range d.items {
		if d.items[n].Key != a.Key {
			continue
		}
		i := &d.items[n]
		switch a.Kind {
		case "summary":
			i.Fields.Summary = a.Value
		case "description":
			i.Fields.Description, _ = json.Marshal(a.Value)
		case "status":
			i.Fields.Status.Name = a.Value
		case "assign":
			i.Fields.Assignee.DisplayName = a.Value
		case "labels":
			i.Fields.Labels = strings.Split(a.Value, ",")
		case "type":
			i.Fields.Type.Name = a.Value
		case "comment":
			body, _ := json.Marshal(a.Value)
			i.Fields.Comments.Comments = append(i.Fields.Comments.Comments, Comment{Author: User{DisplayName: "You (demo)"}, Body: body})
			i.Fields.Comments.Total++
		case "link", "sprint":
			return fmt.Errorf("%s is not simulated in demo mode", a.Kind)
		default:
			return fmt.Errorf("unknown action %s", a.Kind)
		}
		return nil
	}
	return fmt.Errorf("unknown demo ticket %s", a.Key)
}
