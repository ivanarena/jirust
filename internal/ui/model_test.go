package ui

import (
	"strings"
	"testing"
	"time"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/x/ansi"
	"github.com/ivanarena/jiraui/internal/cache"
	"github.com/ivanarena/jiraui/internal/config"
	"github.com/ivanarena/jiraui/internal/jira"
)

func key(s string) tea.KeyMsg {
	switch s {
	case "enter":
		return tea.KeyMsg{Type: tea.KeyEnter}
	case "esc":
		return tea.KeyMsg{Type: tea.KeyEsc}
	case "tab":
		return tea.KeyMsg{Type: tea.KeyTab}
	case "down":
		return tea.KeyMsg{Type: tea.KeyDown}
	case "right":
		return tea.KeyMsg{Type: tea.KeyRight}
	case "ctrl+s":
		return tea.KeyMsg{Type: tea.KeyCtrlS}
	}
	return tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune(s)}
}
func loaded(t *testing.T) *Model {
	t.Helper()
	d := jira.NewDemo()
	store, _ := cache.New("test", true)
	m := New(d, store, config.Config{}, false, 100, "")
	m.Update(tea.WindowSizeMsg{Width: 160, Height: 45})
	items, err := d.Search("", 100)
	if err != nil {
		t.Fatal(err)
	}
	m.Update(listMsg{seq: 0, items: items})
	for _, i := range items {
		m.details[i.Key] = record{issue: i, updated: time.Now()}
	}
	return m
}
func TestAsyncSelectionAndStaleLists(t *testing.T) {
	m := loaded(t)
	if m.detailKey != "DEV-101" {
		t.Fatal("selected state not committed")
	}
	m.Update(key("down"))
	if m.selected != 1 || m.detailKey != "DEV-102" {
		t.Fatal(m.selected, m.detailKey)
	}
	before := len(m.items)
	m.seq = 7
	m.Update(listMsg{seq: 6, items: nil})
	if len(m.items) != before {
		t.Fatal("stale response changed list")
	}
	_, cmd := m.Update(selectTick{key: "DEV-101", seq: m.selectionSeq - 1})
	if cmd != nil {
		t.Fatal("stale selection performed network request")
	}
}
func TestLocalSearchThenRemoteSearch(t *testing.T) {
	m := loaded(t)
	m.Update(key("/"))
	m.searchInput.SetValue("cached-no-match")
	m.Update(key(""))
	if len(m.visible) != 0 {
		t.Fatal("local filter failed")
	}
	_, cmd := m.Update(key("enter"))
	if cmd == nil || m.local != "" || m.search != "cached-no-match" {
		t.Fatal("remote search not scheduled or local results hidden")
	}
	if !strings.Contains(m.query(), `text ~ "cached-no-match"`) {
		t.Fatal(m.query())
	}
}
func TestLinkedNavigationBack(t *testing.T) {
	m := loaded(t)
	m.openInternal("DEV-102")
	if m.detailKey != "DEV-102" || len(m.history) != 1 {
		t.Fatal(m.detailKey, m.history)
	}
	m.Update(key("esc"))
	if m.detailKey != "DEV-101" || len(m.history) != 0 {
		t.Fatal(m.detailKey, m.history)
	}
}
func TestReadOnlyBlocksActionsAndConfirmation(t *testing.T) {
	m := loaded(t)
	m.readOnly = true
	m.Update(key("t"))
	if m.dialog != nil || !m.isError {
		t.Fatal("read-only action opened")
	}
	m.confirm(jira.Action{Kind: "status", Key: "DEV-101", Value: "Done"})
	_, cmd := m.Update(key("y"))
	if cmd != nil || m.mutating {
		t.Fatal("read-only confirmation executed")
	}
}
func TestMutationRequiresExplicitConfirmation(t *testing.T) {
	m := loaded(t)
	m.Update(key("a"))
	if m.dialog == nil || m.dialog.kind != "assignments" {
		t.Fatal("no assignment picker")
	}
	_, cmd := m.Update(key("enter"))
	if cmd != nil || m.dialog.kind != "confirm" || m.mutating {
		t.Fatal("picker executed an update")
	}
	m.Update(key("enter"))
	if m.mutating {
		t.Fatal("enter confirmed mutation")
	}
	_, cmd = m.Update(key("y"))
	if cmd == nil || !m.mutating || m.dialog != nil {
		t.Fatal("explicit confirmation failed")
	}
	batch, ok := cmd().(tea.BatchMsg)
	if !ok || len(batch) != 2 {
		t.Fatal("unexpected batch")
	}
	message := batch[0]()
	a, ok := message.(actionMsg)
	if !ok || a.err != nil {
		t.Fatal(message)
	}
}
func TestStatusPickerUsesActualLoadedNames(t *testing.T) {
	m := loaded(t)
	m.Update(key("t"))
	if m.dialog == nil || m.dialog.kind != "statuses" {
		t.Fatal("no status picker")
	}
	found := false
	for _, s := range m.dialog.values {
		found = found || s == "Review"
	}
	if !found {
		t.Fatal(m.dialog.values)
	}
}
func TestBoardNavigationAndGrouping(t *testing.T) {
	m := loaded(t)
	names, groups := m.statusGroups()
	if names[0] != "To Do" || names[len(names)-1] != "Done" {
		t.Fatal(names)
	}
	m.boardMode = true
	m.selected = groups[names[0]][0]
	m.Update(key("right"))
	i, _ := m.selectedIssue()
	if i.Fields.Status.Name == "To Do" {
		t.Fatal("column navigation failed")
	}
}
func TestViewportAndDialogFitTerminal(t *testing.T) {
	for _, size := range [][2]int{{60, 18}, {83, 30}, {120, 24}, {163, 56}, {200, 60}} {
		for _, mode := range []string{"list", "detail", "board", "filters", "edit", "help", "error"} {
			m := loaded(t)
			m.Update(tea.WindowSizeMsg{Width: size[0], Height: size[1]})
			switch mode {
			case "detail":
				m.focus = "detail"
			case "board":
				m.boardMode = true
			case "filters":
				m.beginFilters()
				m.dialog.index = 7
			case "edit":
				m.beginArea("Description", "A long line with words and Unicode: 日本語 🙂", jira.Action{Kind: "description", Key: "DEV-101"})
			case "help":
				m.help = true
			case "error":
				m.beginError(strings.Repeat("An example CLI error with details.\n", 40))
			}
			view := m.View()
			rows := strings.Split(view, "\n")
			if len(rows) > size[1] {
				t.Fatalf("%v %s uses %d rows", size, mode, len(rows))
			}
			for _, row := range rows {
				if width := ansi.StringWidth(row); width > size[0] {
					t.Fatalf("%v %s width %d: %q", size, mode, width, ansi.Strip(row))
				}
			}
		}
	}
}
func TestMouseSelectAndInternalLink(t *testing.T) {
	m := loaded(t)
	side, _, _ := m.widths()
	m.View()
	m.Update(tea.MouseMsg(tea.MouseEvent{X: side + 3, Y: 7, Button: tea.MouseButtonLeft, Action: tea.MouseActionPress}))
	if m.selected != 1 {
		t.Fatal("mouse row selection failed")
	}
	m.detailKey = "DEV-101"
	m.focus = "detail"
	m.detailFor = ""
	m.View()
	side, list, _ := m.widths()
	for n, target := range m.lineLinks {
		if target.Key == "DEV-102" {
			m.viewport.SetYOffset(n)
			m.Update(tea.MouseMsg(tea.MouseEvent{X: side + list + 2, Y: 4 + n - m.viewport.YOffset, Button: tea.MouseButtonLeft, Action: tea.MouseActionPress}))
			if m.detailKey != "DEV-102" {
				t.Fatal("internal link click failed")
			}
			return
		}
	}
	t.Fatal("no clickable internal link")
}
func TestErrorsDoNotClaimSuccess(t *testing.T) {
	m := loaded(t)
	m.mutating = true
	m.Update(actionMsg{action: jira.Action{Kind: "status", Key: "DEV-101"}, err: errTest{}})
	if !m.isError || m.mutating || strings.Contains(m.status, "Updated") {
		t.Fatal(m.status)
	}
}

type errTest struct{}

func (errTest) Error() string { return "permission denied" }
func BenchmarkCachedNavigation(b *testing.B) {
	d := jira.NewDemo()
	store, _ := cache.New("bench", true)
	m := New(d, store, config.Config{}, true, 100, "")
	m.width = 160
	m.height = 45
	m.items, _ = d.Search("", 100)
	m.rebuildVisible()
	for _, i := range m.items {
		m.details[i.Key] = record{issue: i, updated: time.Now()}
	}
	b.ResetTimer()
	for n := 0; n < b.N; n++ {
		m.Update(key("down"))
		m.Update(key("up"))
	}
}
