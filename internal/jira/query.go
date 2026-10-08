package jira

import (
	"fmt"
	"strconv"
	"strings"
)

type Filters struct {
	Project  string `json:"project,omitempty"`
	Assignee string `json:"assignee,omitempty"`
	Status   string `json:"status,omitempty"`
	Reporter string `json:"reporter,omitempty"`
	Type     string `json:"type,omitempty"`
	Priority string `json:"priority,omitempty"`
	Labels   string `json:"labels,omitempty"`
	Sprint   string `json:"sprint,omitempty"`
}

func quote(s string) string {
	s = strings.ReplaceAll(s, `\`, `\\`)
	s = strings.ReplaceAll(s, `"`, `\"`)
	return `"` + s + `"`
}
func values(field, value string) string {
	var parts []string
	for _, p := range strings.Split(value, ",") {
		if p = strings.TrimSpace(p); p != "" {
			if id, err := strconv.Atoi(p); field == "sprint" && err == nil && id > 0 {
				parts = append(parts, strconv.Itoa(id))
			} else {
				parts = append(parts, quote(p))
			}
		}
	}
	if len(parts) == 0 {
		return ""
	}
	if len(parts) == 1 {
		return field + " = " + parts[0]
	}
	return field + " IN (" + strings.Join(parts, ", ") + ")"
}
func Query(view string, f Filters, search, raw string) string {
	var clauses []string
	if strings.TrimSpace(raw) != "" {
		return raw
	}
	if f.Project != "" {
		clauses = append(clauses, values("project", f.Project))
	}
	if view == "mine" && f.Assignee == "" {
		clauses = append(clauses, "assignee = currentUser()")
	}
	if view == "mine" {
		clauses = append(clauses, "statusCategory != Done")
	}
	if view == "backlog" {
		clauses = append(clauses, "statusCategory != Done", "(sprint IS EMPTY OR sprint IN futureSprints())")
	}
	if view == "sprint" && f.Sprint == "" {
		clauses = append(clauses, "sprint IN openSprints()")
	}
	for _, pair := range [][2]string{{"assignee", f.Assignee}, {"reporter", f.Reporter}} {
		if pair[1] == "" {
			continue
		}
		switch strings.ToLower(pair[1]) {
		case "@me", "me":
			clauses = append(clauses, pair[0]+" = currentUser()")
		case "unassigned", "none":
			clauses = append(clauses, pair[0]+" IS EMPTY")
		default:
			clauses = append(clauses, values(pair[0], pair[1]))
		}
	}
	for _, pair := range [][2]string{{"status", f.Status}, {"issuetype", f.Type}, {"priority", f.Priority}, {"labels", f.Labels}, {"sprint", f.Sprint}} {
		if pair[1] != "" {
			clauses = append(clauses, values(pair[0], pair[1]))
		}
	}
	if search = strings.TrimSpace(search); search != "" {
		if IssueKey.MatchString(search) {
			clauses = append(clauses, "key = "+quote(strings.ToUpper(search)))
		} else {
			clauses = append(clauses, "text ~ "+quote(search))
		}
	}
	if len(clauses) == 0 {
		clauses = append(clauses, "project IS NOT EMPTY")
	}
	order := "updated DESC"
	if view == "backlog" {
		order = "Rank ASC"
	}
	return fmt.Sprintf("%s ORDER BY %s", strings.Join(clauses, " AND "), order)
}
