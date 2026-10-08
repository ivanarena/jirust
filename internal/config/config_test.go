package config

import (
	"os"
	"testing"

	"github.com/ivanarena/jiraui/internal/jira"
)

func TestRoundTripPrivatePreferences(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	want := Config{Filters: jira.Filters{Project: "DEV", Assignee: "@me"}, Saved: []Preset{{Name: "My bugs", JQL: "issuetype = Bug"}}}
	if err := Save(want); err != nil {
		t.Fatal(err)
	}
	got, err := Load()
	if err != nil || got.Filters.Project != "DEV" || len(got.Saved) != 1 {
		t.Fatalf("%+v %v", got, err)
	}
	p, _ := Path()
	info, err := os.Stat(p)
	if err != nil || info.Mode().Perm() != 0600 {
		t.Fatal("preferences not private")
	}
}
