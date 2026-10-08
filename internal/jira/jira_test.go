package jira

import (
	"encoding/json"
	"errors"
	"os"
	"reflect"
	"strings"
	"testing"
)

type fakeRunner struct {
	binary  string
	args    []string
	reply   []byte
	err     error
	inspect func([]string)
}

func (r *fakeRunner) Run(binary string, args ...string) ([]byte, error) {
	r.binary = binary
	r.args = append([]string{}, args...)
	if r.inspect != nil {
		r.inspect(args)
	}
	return r.reply, r.err
}

const sample = `{"key":"DEV-1","self":"https://example.atlassian.net/rest/api/3/issue/1","fields":{"summary":"Hello","status":{"id":"1","name":"In Progress","statusCategory":{"id":4,"key":"indeterminate","name":"In Progress"}},"assignee":null,"description":{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Read "},{"type":"text","text":"the docs","marks":[{"type":"link","attrs":{"href":"https://example.com/docs"}}]}]}]},"customfield_10001":7,"issuelinks":[{"type":{"outward":"blocks"},"outwardIssue":{"key":"DEV-2","fields":{"summary":"Another ticket"}}}]},"names":{"customfield_10001":"Story points"}}`

func TestJSONAndADF(t *testing.T) {
	var i Issue
	if err := json.Unmarshal([]byte(sample), &i); err != nil {
		t.Fatal(err)
	}
	if Text(i.Fields.Description) != "Read the docs" {
		t.Fatal(Text(i.Fields.Description))
	}
	if i.Fields.Assignee.String() != "Unassigned" {
		t.Fatal("null user")
	}
	b, err := json.Marshal(i)
	if err != nil {
		t.Fatal(err)
	}
	var cached Issue
	if err = json.Unmarshal(b, &cached); err != nil {
		t.Fatal(err)
	}
	if string(cached.RawFields["customfield_10001"]) != "7" {
		t.Fatal("custom field lost from cache")
	}
	if !reflect.DeepEqual(CustomFields(i), []string{"Story points: 7"}) {
		t.Fatal(CustomFields(i))
	}
}
func TestTargetsAndURLs(t *testing.T) {
	var i Issue
	_ = json.Unmarshal([]byte(sample), &i)
	targets := Targets(i)
	var internal, external bool
	for _, x := range targets {
		internal = internal || x.Key == "DEV-2"
		external = external || x.URL == "https://example.com/docs"
	}
	if !internal || !external {
		t.Fatal(targets)
	}
	if WebURL(i) != "https://example.atlassian.net/browse/DEV-1" {
		t.Fatal(WebURL(i))
	}
	for _, s := range []string{"javascript:alert(1)", "file:///etc/passwd", "https://user:password@example.com", "https://example.com\n"} {
		if ValidateURL(s) == nil {
			t.Fatalf("accepted %q", s)
		}
	}
}
func TestSanitizeTerminal(t *testing.T) {
	s := Safe("a\x1b[31mb\x1b[0m\x1b]52;c;secret\a\x00")
	if strings.ContainsAny(s, "\x1b\x00\a") {
		t.Fatalf("escape survives %q", s)
	}
	if !strings.Contains(s, "ab") {
		t.Fatal(s)
	}
}
func TestQuery(t *testing.T) {
	q := Query("mine", Filters{Project: "DEV", Status: "To Do,In Progress", Assignee: "@me"}, "hello", "")
	for _, want := range []string{`project = "DEV"`, `status IN ("To Do", "In Progress")`, `assignee = currentUser()`, `text ~ "hello"`, `ORDER BY updated DESC`} {
		if !strings.Contains(q, want) {
			t.Fatalf("%q missing from %s", want, q)
		}
	}
	if strings.Count(q, "assignee = currentUser()") != 1 {
		t.Fatal("duplicated assignee")
	}
	raw := "project = OTHER ORDER BY Rank ASC"
	if Query("mine", Filters{Project: "DEV"}, "", raw) != raw {
		t.Fatal("raw JQL modified")
	}
	q = Query("all", Filters{Assignee: `x" OR project = "OTHER`}, "", "")
	if !strings.Contains(q, `assignee = "x\" OR project = \"OTHER"`) {
		t.Fatal(q)
	}
	if !strings.Contains(Query("backlog", Filters{}, "", ""), "ORDER BY Rank ASC") {
		t.Fatal("backlog isn't ranked")
	}
}
func TestSearchArgumentsAreNotAShell(t *testing.T) {
	r := &fakeRunner{reply: []byte("[" + sample + "]")}
	c := CLI{Kind: "acli", Binary: "/test/acli", Runner: r}
	query := `summary ~ "$(touch /tmp/never)"`
	items, err := c.Search(query, 20)
	if err != nil || len(items) != 1 {
		t.Fatalf("%v %v", items, err)
	}
	if r.binary != "/test/acli" || r.args[4] != query {
		t.Fatal(r.args)
	}
	for _, arg := range r.args {
		if arg == "sh" || arg == "-c" {
			t.Fatal("shell execution")
		}
	}
}
func TestCollectionEnvelopes(t *testing.T) {
	for _, data := range []string{"[" + sample + "]", `{"issues":[` + sample + `]}`, `{"values":[` + sample + `]}`} {
		items, err := list[Issue]([]byte(data), "issues", "values")
		if err != nil || len(items) != 1 {
			t.Fatalf("%v %v", items, err)
		}
	}
	if _, err := list[Issue]([]byte(`{"unexpected":[]}`), "issues"); err == nil {
		t.Fatal("unknown envelope accepted")
	}
}
func TestACLIActionArguments(t *testing.T) {
	tests := []struct {
		a    Action
		want []string
	}{
		{Action{Kind: "status", Key: "DEV-1", Value: "In Progress"}, []string{"jira", "workitem", "transition", "--key", "DEV-1", "--status", "In Progress", "--yes", "--json"}},
		{Action{Kind: "assign", Key: "DEV-1", Value: "unassigned"}, []string{"jira", "workitem", "assign", "--key", "DEV-1", "--yes", "--json", "--remove-assignee"}},
		{Action{Kind: "summary", Key: "DEV-1", Value: "Literal $(not-a-shell)"}, []string{"jira", "workitem", "edit", "--key", "DEV-1", "--summary", "Literal $(not-a-shell)", "--yes", "--json"}},
		{Action{Kind: "link", Key: "DEV-1", Value: "DEV-2", Extra: "blocks"}, []string{"jira", "workitem", "link", "create", "--out", "DEV-1", "--in", "DEV-2", "--type", "blocks", "--yes"}},
	}
	for _, tt := range tests {
		r := &fakeRunner{reply: []byte(`{"success":true}`)}
		c := CLI{Kind: "acli", Binary: "acli", Runner: r}
		if err := c.Apply(tt.a); err != nil {
			t.Fatal(err)
		}
		if !reflect.DeepEqual(r.args, tt.want) {
			t.Fatalf("%s: %v", tt.a.Kind, r.args)
		}
	}
}
func TestBodyFilesArePrivateAndRemoved(t *testing.T) {
	for _, kind := range []string{"description", "comment"} {
		t.Run(kind, func(t *testing.T) {
			var path string
			r := &fakeRunner{reply: []byte(`{}`), inspect: func(args []string) {
				for n, a := range args {
					if a == "--body-file" || a == "--description-file" {
						path = args[n+1]
					}
				}
				info, err := os.Stat(path)
				if err != nil {
					t.Fatal(err)
				}
				if info.Mode().Perm() != 0600 {
					t.Fatal(info.Mode())
				}
				b, _ := os.ReadFile(path)
				if string(b) != "private body" {
					t.Fatal(string(b))
				}
			}}
			c := CLI{Kind: "acli", Binary: "acli", Runner: r}
			if err := c.Apply(Action{Kind: kind, Key: "DEV-1", Value: "private body"}); err != nil {
				t.Fatal(err)
			}
			if _, err := os.Stat(path); !os.IsNotExist(err) {
				t.Fatal("body file not removed")
			}
		})
	}
}
func TestInvalidActionsAndFailedReceipts(t *testing.T) {
	r := &fakeRunner{}
	c := CLI{Kind: "acli", Binary: "acli", Runner: r}
	for _, a := range []Action{{Kind: "summary", Key: "--help", Value: "x"}, {Kind: "status", Key: "DEV-1", Value: ""}, {Kind: "sprint", Key: "DEV-1", Value: "123"}, {Kind: "link", Key: "DEV-1", Value: "DEV-2"}} {
		if c.Apply(a) == nil {
			t.Fatal("invalid action accepted", a)
		}
	}
	if len(r.args) > 0 {
		t.Fatal("invalid input executed")
	}
	for _, reply := range []string{`{"success":false}`, `{"errors":["denied"]}`, `{"results":[{"failed":1}]}`} {
		r.reply = []byte(reply)
		if c.Apply(Action{Kind: "status", Key: "DEV-1", Value: "Done"}) == nil {
			t.Fatal("failed receipt accepted")
		}
	}
	r.err = errors.New("network failed")
	if c.Apply(Action{Kind: "status", Key: "DEV-1", Value: "Done"}) == nil {
		t.Fatal("CLI error swallowed")
	}
}
func TestScopeUnauthenticated(t *testing.T) {
	c := CLI{Kind: "acli", Binary: "acli", Runner: &fakeRunner{err: errors.New("not authenticated")}}
	if c.Scope() != "" {
		t.Fatal("unauthenticated account has a disk namespace")
	}
}
