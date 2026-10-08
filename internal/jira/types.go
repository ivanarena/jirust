package jira

import (
	"encoding/json"
	"fmt"
	"net/url"
	"regexp"
	"sort"
	"strings"

	"github.com/charmbracelet/x/ansi"
)

type Named struct {
	ID   any    `json:"id"`
	Name string `json:"name"`
	Key  string `json:"key"`
}
type User struct {
	AccountID   string `json:"accountId"`
	DisplayName string `json:"displayName"`
	Email       string `json:"emailAddress"`
}

func (u User) String() string {
	if u.DisplayName != "" {
		return Safe(u.DisplayName)
	}
	if u.Email != "" {
		return Safe(u.Email)
	}
	return "Unassigned"
}

type Status struct {
	ID       string `json:"id"`
	Name     string `json:"name"`
	Category Named  `json:"statusCategory"`
}
type Comment struct {
	ID      string          `json:"id"`
	Author  User            `json:"author"`
	Created string          `json:"created"`
	Body    json.RawMessage `json:"body"`
}
type Comments struct {
	Comments []Comment `json:"comments"`
	Total    int       `json:"total"`
}
type Attachment struct {
	Filename string `json:"filename"`
	Content  string `json:"content"`
	Size     int64  `json:"size"`
}
type Relation struct {
	Type struct {
		Name    string `json:"name"`
		Inward  string `json:"inward"`
		Outward string `json:"outward"`
	} `json:"type"`
	InwardIssue  *Issue `json:"inwardIssue"`
	OutwardIssue *Issue `json:"outwardIssue"`
}
type Fields struct {
	Summary     string          `json:"summary"`
	Description json.RawMessage `json:"description"`
	Status      Status          `json:"status"`
	Assignee    User            `json:"assignee"`
	Reporter    User            `json:"reporter"`
	Priority    Named           `json:"priority"`
	Type        Named           `json:"issuetype"`
	Project     Named           `json:"project"`
	Labels      []string        `json:"labels"`
	Parent      *Issue          `json:"parent"`
	Subtasks    []Issue         `json:"subtasks"`
	Links       []Relation      `json:"issuelinks"`
	Comments    Comments        `json:"comment"`
	Attachments []Attachment    `json:"attachment"`
	Created     string          `json:"created"`
	Updated     string          `json:"updated"`
	DueDate     string          `json:"duedate"`
}
type Transition struct {
	ID   string `json:"id"`
	Name string `json:"name"`
	To   Status `json:"to"`
}
type Issue struct {
	ID          string                     `json:"id"`
	Key         string                     `json:"key"`
	Self        string                     `json:"self"`
	Fields      Fields                     `json:"fields"`
	Names       map[string]string          `json:"names"`
	Transitions []Transition               `json:"transitions"`
	RawFields   map[string]json.RawMessage `json:"-"`
}

func (i *Issue) UnmarshalJSON(data []byte) error {
	type alias Issue
	var a alias
	if err := json.Unmarshal(data, &a); err != nil {
		return err
	}
	var raw struct {
		Fields map[string]json.RawMessage `json:"fields"`
	}
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	*i = Issue(a)
	i.RawFields = raw.Fields
	return nil
}

// Re-marshal untouched custom fields so a cached detail remains complete.
func (i Issue) MarshalJSON() ([]byte, error) {
	type alias Issue
	data, err := json.Marshal(alias(i))
	if err != nil {
		return nil, err
	}
	var obj map[string]json.RawMessage
	if err := json.Unmarshal(data, &obj); err != nil {
		return nil, err
	}
	var fields map[string]json.RawMessage
	if err := json.Unmarshal(obj["fields"], &fields); err != nil {
		return nil, err
	}
	for k, v := range i.RawFields {
		if _, exists := fields[k]; !exists {
			fields[k] = v
		}
	}
	obj["fields"], err = json.Marshal(fields)
	if err != nil {
		return nil, err
	}
	return json.Marshal(obj)
}

type Board struct {
	ID   int    `json:"id"`
	Name string `json:"name"`
	Type string `json:"type"`
}
type Sprint struct {
	ID    int    `json:"id"`
	Name  string `json:"name"`
	State string `json:"state"`
}
type Action struct {
	Kind  string
	Key   string
	Value string
	Extra string
}
type Backend interface {
	Name() string
	Search(string, int) ([]Issue, error)
	Get(string) (Issue, error)
	Comments(string) ([]Comment, error)
	Boards(string) ([]Board, error)
	Sprints(int) ([]Sprint, error)
	SprintIssues(int, int, string, int) ([]Issue, error)
	Apply(Action) error
}

var IssueKey = regexp.MustCompile(`^[A-Za-z][A-Za-z0-9_]*-[0-9]+$`)
var IssueKeys = regexp.MustCompile(`\b[A-Z][A-Z0-9_]*-[0-9]+\b`)
var URLs = regexp.MustCompile(`https?://[^\s<>"\x00-\x1f]+`)

func Safe(s string) string {
	s = ansi.Strip(s)
	return strings.Map(func(r rune) rune {
		if r == '\n' || r == '\t' {
			return r
		}
		if r < 32 || (r >= 127 && r <= 159) {
			return -1
		}
		return r
	}, s)
}
func SafeLine(s string) string { return strings.Join(strings.Fields(Safe(s)), " ") }
func WebURL(i Issue) string {
	u, err := url.Parse(i.Self)
	if err != nil || (u.Scheme != "https" && u.Scheme != "http") || u.Host == "" {
		return ""
	}
	u.User = nil
	u.Path = "/browse/" + i.Key
	u.RawPath = ""
	u.RawQuery = ""
	u.Fragment = ""
	return u.String()
}
func ValidateURL(s string) error {
	u, err := url.Parse(s)
	if err != nil || u.Host == "" || (u.Scheme != "https" && u.Scheme != "http") || u.User != nil {
		return fmt.Errorf("only ordinary HTTP(S) links may be opened")
	}
	return nil
}

// Text renders Jira's ADF recursively without HTML or terminal escape execution.
func Text(raw json.RawMessage) string {
	if len(raw) == 0 || string(raw) == "null" {
		return ""
	}
	var plain string
	if json.Unmarshal(raw, &plain) == nil {
		return Safe(plain)
	}
	var v any
	if json.Unmarshal(raw, &v) != nil {
		return ""
	}
	return strings.TrimSpace(Safe(adf(v)))
}
func adf(v any) string {
	switch n := v.(type) {
	case string:
		return n
	case []any:
		var b strings.Builder
		for _, c := range n {
			b.WriteString(adf(c))
		}
		return b.String()
	case map[string]any:
		kind, _ := n["type"].(string)
		attrs, _ := n["attrs"].(map[string]any)
		if kind == "text" {
			s, _ := n["text"].(string)
			return s
		}
		if kind == "hardBreak" {
			return "\n"
		}
		if kind == "mention" {
			s, _ := attrs["text"].(string)
			return s
		}
		if kind == "emoji" {
			if s, ok := attrs["text"].(string); ok {
				return s
			}
			s, _ := attrs["shortName"].(string)
			return s
		}
		if kind == "inlineCard" || kind == "blockCard" {
			s, _ := attrs["url"].(string)
			return s
		}
		if kind == "media" {
			return "[attachment]"
		}
		body := adf(n["content"])
		switch kind {
		case "paragraph", "heading", "codeBlock", "blockquote", "tableRow":
			return body + "\n"
		case "listItem":
			return "• " + body
		case "tableCell", "tableHeader":
			return body + "  "
		}
		return body
	}
	return ""
}

type Target struct {
	Label string
	Key   string
	URL   string
}

// A same-site /browse/ URL is an internal ticket link; other sites remain external.
func URLTarget(i Issue, s string) Target {
	t := Target{Label: s, URL: s}
	u, err := url.Parse(s)
	site, siteErr := url.Parse(i.Self)
	if err != nil || siteErr != nil || ValidateURL(s) != nil || !strings.EqualFold(u.Host, site.Host) {
		return t
	}
	key := strings.TrimSuffix(strings.TrimPrefix(u.Path, "/browse/"), "/")
	if strings.HasPrefix(u.Path, "/browse/") && IssueKey.MatchString(key) {
		t.Key = strings.ToUpper(key)
		t.URL = ""
	}
	return t
}
func Targets(i Issue) []Target {
	var out []Target
	seen := map[string]bool{}
	add := func(t Target) {
		id := t.Key + t.URL
		if id == "" || seen[id] {
			return
		}
		seen[id] = true
		t.Label = SafeLine(t.Label)
		out = append(out, t)
	}
	if i.Fields.Parent != nil {
		p := i.Fields.Parent
		add(Target{Label: "Parent · " + p.Key + " " + p.Fields.Summary, Key: p.Key})
	}
	for _, s := range i.Fields.Subtasks {
		add(Target{Label: "Subtask · " + s.Key + " " + s.Fields.Summary, Key: s.Key})
	}
	for _, l := range i.Fields.Links {
		if l.InwardIssue != nil {
			s := l.InwardIssue
			add(Target{Label: l.Type.Inward + " · " + s.Key + " " + s.Fields.Summary, Key: s.Key})
		}
		if l.OutwardIssue != nil {
			s := l.OutwardIssue
			add(Target{Label: l.Type.Outward + " · " + s.Key + " " + s.Fields.Summary, Key: s.Key})
		}
	}
	text := Text(i.Fields.Description)
	for _, c := range i.Fields.Comments.Comments {
		text += "\n" + Text(c.Body)
	}
	for _, key := range IssueKeys.FindAllString(text, -1) {
		if key != i.Key {
			add(Target{Label: "Mention · " + key, Key: key})
		}
	}
	var walk func(any)
	walk = func(v any) {
		switch x := v.(type) {
		case []any:
			for _, c := range x {
				walk(c)
			}
		case map[string]any:
			if a, ok := x["attrs"].(map[string]any); ok {
				for _, k := range []string{"href", "url"} {
					if s, ok := a[k].(string); ok && ValidateURL(s) == nil {
						add(URLTarget(i, s))
					}
				}
			}
			for _, c := range x {
				walk(c)
			}
		}
	}
	var doc any
	_ = json.Unmarshal(i.Fields.Description, &doc)
	walk(doc)
	for _, c := range i.Fields.Comments.Comments {
		var d any
		_ = json.Unmarshal(c.Body, &d)
		walk(d)
	}
	for _, s := range URLs.FindAllString(text, -1) {
		s = strings.TrimRight(s, ".,;)]}")
		if ValidateURL(s) == nil {
			add(URLTarget(i, s))
		}
	}
	for _, a := range i.Fields.Attachments {
		if ValidateURL(a.Content) == nil {
			add(Target{Label: "Attachment · " + a.Filename, URL: a.Content})
		}
	}
	if u := WebURL(i); u != "" {
		add(Target{Label: "Open " + i.Key + " in Jira", URL: u})
	}
	return out
}
func CustomFields(i Issue) []string {
	var out []string
	for key, raw := range i.RawFields {
		if !strings.HasPrefix(key, "customfield_") || string(raw) == "null" || string(raw) == "[]" || string(raw) == "\"\"" {
			continue
		}
		name := i.Names[key]
		if name == "" {
			name = key
		}
		value := Text(raw)
		if value == "" {
			var v any
			if json.Unmarshal(raw, &v) == nil {
				b, _ := json.Marshal(v)
				value = string(b)
			}
		}
		out = append(out, SafeLine(name)+": "+SafeLine(value))
	}
	sort.Strings(out)
	return out
}
