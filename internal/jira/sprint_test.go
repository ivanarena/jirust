package jira

import (
	"reflect"
	"testing"
)

func TestACLIBoardSprintCommands(t *testing.T) {
	r := &fakeRunner{reply: []byte(`{"values":[{"id":123,"name":"Synthetic board","type":"scrum"}]}`)}
	c := CLI{Kind: "acli", Binary: "acli", Runner: r}
	boards, err := c.Boards("DEV")
	if err != nil || len(boards) != 1 {
		t.Fatalf("%v %v", boards, err)
	}
	if !reflect.DeepEqual(r.args, []string{"jira", "board", "search", "--json", "--limit", "100", "--project", "DEV"}) {
		t.Fatal(r.args)
	}
	r.reply = []byte(`{"values":[{"id":7,"name":"Synthetic sprint","state":"active"}]}`)
	sprints, err := c.Sprints(123)
	if err != nil || len(sprints) != 1 {
		t.Fatalf("%v %v", sprints, err)
	}
	r.reply = []byte(`{"issues":[` + sample + `]}`)
	issues, err := c.SprintIssues(123, 7, "sprint = 7 ORDER BY updated DESC", 20)
	if err != nil || len(issues) != 1 {
		t.Fatalf("%v %v", issues, err)
	}
	want := []string{"jira", "sprint", "list-workitems", "--board", "123", "--sprint", "7", "--jql", "sprint = 7 ORDER BY updated DESC", "--limit", "20", "--json"}
	if !reflect.DeepEqual(r.args, want) {
		t.Fatal(r.args)
	}
}
func TestJiraSprintMovement(t *testing.T) {
	r := &fakeRunner{}
	c := CLI{Kind: "jira", Binary: "jira", Runner: r}
	if err := c.Apply(Action{Kind: "sprint", Key: "DEV-1", Value: "7"}); err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(r.args, []string{"sprint", "add", "7", "DEV-1"}) {
		t.Fatal(r.args)
	}
}
