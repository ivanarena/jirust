package ui

import (
	"fmt"
	"sort"
	"strconv"
	"strings"

	"github.com/charmbracelet/bubbles/textarea"
	"github.com/charmbracelet/bubbles/textinput"
	"github.com/charmbracelet/bubbles/viewport"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/ivanarena/jiraui/internal/config"
	"github.com/ivanarena/jiraui/internal/jira"
)

func input(value string) textinput.Model {
	i := textinput.New()
	i.CharLimit = 1000
	i.Width = 58
	i.SetValue(value)
	return i
}
func (m *Model) beginInput(kind, title, value string) {
	i := input(value)
	i.Focus()
	m.dialog = &Dialog{kind: kind, title: title, inputs: []textinput.Model{i}}
}
func (m *Model) beginFilters() {
	f := m.cfg.Filters
	values := []string{f.Project, f.Assignee, f.Status, f.Reporter, f.Type, f.Priority, f.Labels, f.Sprint}
	d := &Dialog{kind: "filters", title: "Filters · blank clears · commas combine values", labels: []string{"Project", "Assignee (@me / unassigned / email or ID)", "Status", "Reporter (@me / email or ID)", "Issue type", "Priority", "Labels", "Sprint (ID or name)"}}
	for _, v := range values {
		d.inputs = append(d.inputs, input(v))
	}
	d.inputs[0].Focus()
	m.dialog = d
}
func (m *Model) beginLinks(i jira.Issue) {
	targets := jira.Targets(i)
	if len(targets) == 0 {
		m.notice("Open ticket details first to load links", true)
		return
	}
	d := &Dialog{kind: "links", title: "Links · ticket keys open internally", targets: targets}
	for _, t := range targets {
		d.options = append(d.options, t.Label)
	}
	m.dialog = d
}
func (m *Model) beginAction(key string) tea.Cmd {
	i, ok := m.activeIssue()
	if !ok || i.Key == "" {
		m.notice("Select a ticket first", true)
		return nil
	}
	if r, yes := m.details[i.Key]; yes {
		i = r.issue
	}
	d := &Dialog{action: jira.Action{Key: i.Key}}
	switch key {
	case "e":
		d.kind = "edit"
		d.title = "Edit " + i.Key
		d.options = []string{"Summary", "Description", "Labels (comma-separated)", "Issue type"}
		m.dialog = d
		return nil
	case "a":
		d.kind = "assignments"
		d.title = "Assign " + i.Key
		d.action.Kind = "assign"
		d.options = []string{"Assign to me", "Unassigned"}
		d.values = []string{"@me", "unassigned"}
		seen := map[string]bool{}
		for _, item := range m.items {
			u := item.Fields.Assignee
			value := u.AccountID
			if m.client.Name() == "jira" || value == "" {
				value = u.Email
			}
			if value != "" && !seen[value] {
				seen[value] = true
				d.options = append(d.options, u.String())
				d.values = append(d.values, value)
			}
		}
		d.options = append(d.options, "Other assignee…")
		d.values = append(d.values, "")
		m.dialog = d
		return nil
	case "t":
		d.kind = "statuses"
		d.title = "Transition " + i.Key + " · Jira validates workflow"
		d.action.Kind = "status"
		seen := map[string]bool{}
		for _, tr := range i.Transitions {
			if tr.To.Name != "" {
				seen[tr.To.Name] = true
			}
		}
		if len(seen) == 0 {
			for _, item := range m.items {
				if item.Fields.Status.Name != "" {
					seen[item.Fields.Status.Name] = true
				}
			}
			d.title += " · loaded status suggestions"
		}
		for name := range seen {
			d.values = append(d.values, name)
		}
		sort.Strings(d.values)
		d.options = append(d.options, d.values...)
		d.options = append(d.options, "Enter another status…")
		d.values = append(d.values, "")
		m.dialog = d
		return nil
	case "c":
		m.beginArea("Comment on "+i.Key, "", jira.Action{Kind: "comment", Key: i.Key})
	case "L":
		m.beginInput("action", "Link "+i.Key+" to another ticket", "")
		d = m.dialog
		d.labels = []string{"Target ticket key", "Outward link type (e.g. blocks)"}
		d.inputs = append(d.inputs, input("blocks"))
		d.action = jira.Action{Kind: "link", Key: i.Key}
	case "m":
		if m.client.Name() == "acli" {
			m.notice("acli cannot move tickets to a sprint; configure --backend jira for this action", true)
			return nil
		}
		m.beginInput("action", "Move "+i.Key+" to sprint ID", "")
		m.dialog.action = jira.Action{Kind: "sprint", Key: i.Key}
	}
	return textinput.Blink
}
func (m *Model) beginArea(title, value string, a jira.Action) {
	area := textarea.New()
	area.CharLimit = 100000
	area.SetWidth(max(20, min(76, m.width-10)))
	area.SetHeight(max(5, min(15, m.height-12)))
	area.SetValue(value)
	area.Focus()
	m.dialog = &Dialog{kind: "area", title: title, area: area, action: a}
}
func (m *Model) beginError(message string) {
	v := viewport.New(max(20, min(80, m.width-10)), max(4, m.height-12))
	v.SetContent(wrap(message, v.Width))
	m.dialog = &Dialog{kind: "error", title: "Last CLI error · reload before retrying an update", errorView: v}
}
func (m *Model) persist() error {
	if m.client.Name() == "demo" {
		return nil
	}
	return config.Save(m.cfg)
}
func (m *Model) confirm(a jira.Action) {
	if strings.TrimSpace(a.Value) == "" {
		m.notice("Empty values are rejected; use 'unassigned' to clear an assignee", true)
		return
	}
	m.dialog = &Dialog{kind: "confirm", title: "Confirm update · " + a.Key, action: a}
}
func (m *Model) submitDialog() tea.Cmd {
	d := m.dialog
	switch d.kind {
	case "filters":
		f := &m.cfg.Filters
		values := []*string{&f.Project, &f.Assignee, &f.Status, &f.Reporter, &f.Type, &f.Priority, &f.Labels, &f.Sprint}
		for n, p := range values {
			*p = strings.TrimSpace(d.inputs[n].Value())
		}
		m.dialog = nil
		m.raw = ""
		m.boardID = 0
		m.sprintID = 0
		m.sprintName = ""
		if err := m.persist(); err != nil {
			m.notice("Cannot save filters: "+err.Error(), true)
		}
		return m.reload(true)
	case "jql":
		m.raw = strings.TrimSpace(d.inputs[0].Value())
		m.dialog = nil
		m.boardID = 0
		m.sprintID = 0
		m.sprintName = ""
		return m.reload(true)
	case "save":
		name := strings.TrimSpace(d.inputs[0].Value())
		if name == "" {
			m.notice("Filter name is required", true)
			return nil
		}
		p := config.Preset{Name: jira.SafeLine(name), JQL: m.query()}
		replaced := false
		for n, s := range m.cfg.Saved {
			if s.Name == p.Name {
				m.cfg.Saved[n] = p
				replaced = true
			}
		}
		if !replaced {
			m.cfg.Saved = append(m.cfg.Saved, p)
		}
		m.dialog = nil
		if err := m.persist(); err != nil {
			m.notice(err.Error(), true)
		} else {
			m.notice("Saved filter "+p.Name, false)
		}
		return nil
	case "open":
		key := strings.ToUpper(strings.TrimSpace(d.inputs[0].Value()))
		m.dialog = nil
		return m.openInternal(key)
	case "action":
		a := d.action
		a.Value = strings.TrimSpace(d.inputs[0].Value())
		if len(d.inputs) > 1 {
			a.Extra = strings.TrimSpace(d.inputs[1].Value())
		}
		m.confirm(a)
	case "area":
		a := d.action
		a.Value = d.area.Value()
		m.confirm(a)
	}
	return nil
}
func (m *Model) updateDialog(msg tea.Msg) (tea.Model, tea.Cmd) {
	d := m.dialog
	if mouse, ok := msg.(tea.MouseMsg); ok && len(d.options) > 0 && mouse.Button == tea.MouseButtonLeft && mouse.Action == tea.MouseActionPress {
		_, w, h := m.dialogBody()
		top, left := 2+(m.height-4-h)/2, (m.width-w)/2
		start := max(0, d.index-max(1, m.height-12)+1)
		row := mouse.Y - top - 4 + start
		if mouse.X > left && mouse.X < left+w-1 && mouse.Y >= top+4 && row >= start && row < len(d.options) && mouse.Y < top+h-3 {
			d.index = row
			return m.updateDialog(tea.KeyMsg{Type: tea.KeyEnter})
		}
		return m, nil
	}
	if key, ok := msg.(tea.KeyMsg); ok {
		k := key.String()
		if k == "esc" || k == "ctrl+c" {
			m.dialog = nil
			return m, nil
		}
		if d.kind == "error" {
			if k == "enter" || k == "q" {
				m.dialog = nil
				return m, nil
			}
			var cmd tea.Cmd
			d.errorView, cmd = d.errorView.Update(msg)
			return m, cmd
		}
		if d.kind == "confirm" {
			switch k {
			case "n":
				m.dialog = nil
				return m, nil
			case "y":
				if m.readOnly {
					m.dialog = nil
					m.notice("Read-only mode: update blocked", true)
					return m, nil
				}
				a, client := d.action, m.client
				m.dialog = nil
				m.mutating = true
				m.notice("Updating "+a.Key+"…", false)
				return m, tea.Batch(func() tea.Msg { return actionMsg{a, client.Apply(a)} }, m.spinner.Tick)
			}
			return m, nil
		}
		if len(d.options) > 0 {
			switch k {
			case "j", "down":
				d.index = min(d.index+1, len(d.options)-1)
			case "k", "up":
				d.index = max(d.index-1, 0)
			case "enter":
				switch d.kind {
				case "statuses", "assignments":
					a := d.action
					value := d.values[d.index]
					if value == "" {
						m.beginInput("action", "Enter "+a.Kind+" · "+a.Key, "")
						m.dialog.action = a
						return m, textinput.Blink
					}
					a.Value = value
					m.confirm(a)
					return m, nil
				case "links":
					t := d.targets[d.index]
					m.dialog = nil
					return m, m.openTarget(t)
				case "boards":
					b := d.boards[d.index]
					client := m.client
					m.dialog = nil
					m.loading = true
					return m, tea.Batch(func() tea.Msg { s, err := client.Sprints(b.ID); return sprintsMsg{b, s, err} }, m.spinner.Tick)
				case "sprints":
					s := d.sprints[d.index]
					m.sprintID = s.ID
					m.sprintName = jira.SafeLine(s.Name)
					m.view = "sprint"
					m.sidebar = 3
					m.raw = ""
					m.cfg.Filters.Sprint = strconv.Itoa(s.ID)
					m.dialog = nil
					return m, m.reload(true)
				case "edit":
					i, ok := m.activeIssue()
					if !ok {
						m.dialog = nil
						return m, nil
					}
					if r, yes := m.details[i.Key]; yes {
						i = r.issue
					}
					kind := []string{"summary", "description", "labels", "type"}[d.index]
					a := jira.Action{Kind: kind, Key: i.Key}
					if kind == "description" {
						if _, yes := m.details[i.Key]; !yes {
							m.dialog = nil
							m.notice("Load ticket details before editing its description", true)
							return m, m.detail(i.Key, false)
						}
						m.beginArea("Edit description · "+i.Key, jira.Text(i.Fields.Description), a)
						return m, textarea.Blink
					}
					value := i.Fields.Summary
					if kind == "labels" {
						value = strings.Join(i.Fields.Labels, ",")
					}
					if kind == "type" {
						value = i.Fields.Type.Name
					}
					m.beginInput("action", "Edit "+kind+" · "+i.Key, value)
					m.dialog.action = a
					return m, textinput.Blink
				}
			}
			return m, nil
		}
		if k == "ctrl+s" {
			return m, m.submitDialog()
		}
		if d.kind != "area" {
			if k == "tab" || k == "shift+tab" || k == "enter" {
				if k == "enter" && d.index == len(d.inputs)-1 {
					return m, m.submitDialog()
				}
				d.inputs[d.index].Blur()
				step := 1
				if k == "shift+tab" {
					step = -1
				}
				d.index = (d.index + step + len(d.inputs)) % len(d.inputs)
				d.inputs[d.index].Focus()
				return m, textinput.Blink
			}
		}
	}
	var cmd tea.Cmd
	if d.kind == "area" {
		d.area, cmd = d.area.Update(msg)
	} else if len(d.inputs) > 0 {
		d.inputs[d.index], cmd = d.inputs[d.index].Update(msg)
	}
	return m, cmd
}
func (m Model) dialogBody() (string, int, int) {
	d := m.dialog
	w := min(86, m.width-4)
	inner := w - 4
	var body strings.Builder
	body.WriteString(titleStyle.Render(jira.SafeLine(d.title)) + "\n\n")
	switch {
	case d.kind == "error":
		body.WriteString(d.errorView.View() + "\n\n" + mutedStyle.Render("↑↓ scroll · Esc / Enter close"))
	case d.kind == "confirm":
		body.WriteString(fmt.Sprintf("%s → %s\n\n", d.action.Kind, jira.SafeLine(d.action.Key)))
		preview := jira.Safe(d.action.Value)
		if len([]rune(preview)) > 600 {
			preview = string([]rune(preview)[:600]) + "…"
		}
		body.WriteString(wrap(preview, inner) + "\n")
		if d.action.Extra != "" {
			body.WriteString("Link type: " + jira.SafeLine(d.action.Extra) + "\n")
		}
		body.WriteString("\n" + mutedStyle.Render("y confirms · n / Esc cancels · one ticket only"))
	case len(d.options) > 0:
		rows := max(1, m.height-12)
		start := max(0, d.index-rows+1)
		for n := start; n < min(len(d.options), start+rows); n++ {
			text := truncate(d.options[n], inner-2)
			if n == d.index {
				body.WriteString(selectedStyle.Render("› " + text))
			} else {
				body.WriteString("  " + text)
			}
			body.WriteString("\n")
		}
		body.WriteString("\n" + mutedStyle.Render("↑↓ select · Enter opens · Esc cancels"))
	case d.kind == "area":
		d.area.SetWidth(max(12, inner-2))
		body.WriteString(d.area.View() + "\n\n" + mutedStyle.Render("Ctrl+S reviews update · Esc cancels"))
		if d.action.Kind == "description" {
			body.WriteString("\n" + mutedStyle.Render("Saving replaces rich formatting with plain text."))
		}
	default:
		rows := max(1, (m.height-12)/2)
		start := max(0, d.index-rows+1)
		for n := start; n < min(len(d.inputs), start+rows); n++ {
			d.inputs[n].Width = max(12, inner-4)
			if n < len(d.labels) {
				body.WriteString(mutedStyle.Render(truncate(d.labels[n], inner)) + "\n")
			}
			body.WriteString(d.inputs[n].View() + "\n")
		}
		body.WriteString("\n" + mutedStyle.Render("Tab next · Enter continues · Ctrl+S submits · Esc cancels"))
	}
	content := body.String()
	h := min(m.height-4, strings.Count(content, "\n")+3)
	return content, w, h
}
