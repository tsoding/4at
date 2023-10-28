package main

import (
	"strings"
)
type Cmd int

const (
	Quit Cmd = iota + 1
)

var allowCommands = map[string]Cmd  {
	// Quit commands
	":quit": Quit,
	":q": Quit,
	":exit": Quit,
	":e": Quit,
	":close": Quit,
} 

func IsCommand(text string) (Cmd, bool) {
	text = strings.TrimSpace(text)
	cmd, exists := allowCommands[text]
	return cmd, exists
}
