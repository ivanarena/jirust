package config

import (
	"encoding/json"
	"os"
	"path/filepath"

	"github.com/ivanarena/jiraui/internal/jira"
)

type Preset struct {
	Name string `json:"name"`
	JQL  string `json:"jql"`
}
type Config struct {
	Filters jira.Filters `json:"filters"`
	Saved   []Preset     `json:"saved,omitempty"`
}

func Path() (string, error) {
	root, err := os.UserConfigDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(root, "jiraui", "config.json"), nil
}
func Load() (Config, error) {
	var c Config
	p, err := Path()
	if err != nil {
		return c, err
	}
	b, err := os.ReadFile(p)
	if os.IsNotExist(err) {
		return c, nil
	}
	if err != nil {
		return c, err
	}
	err = json.Unmarshal(b, &c)
	return c, err
}
func Save(c Config) error {
	p, err := Path()
	if err != nil {
		return err
	}
	if err = os.MkdirAll(filepath.Dir(p), 0700); err != nil {
		return err
	}
	b, err := json.MarshalIndent(c, "", "  ")
	if err != nil {
		return err
	}
	f, err := os.CreateTemp(filepath.Dir(p), ".config-*")
	if err != nil {
		return err
	}
	defer os.Remove(f.Name())
	if _, err = f.Write(b); err != nil {
		f.Close()
		return err
	}
	if err = f.Close(); err != nil {
		return err
	}
	return os.Rename(f.Name(), p)
}
