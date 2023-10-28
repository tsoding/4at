package main

import (
	"testing"
)

type testCase struct {
	Text string
	CommandExpected Cmd
	IsCommandExpected bool
}

func TestIsCommand(t *testing.T) {
	
	tests := []testCase {
		{
			Text: "hi",
			CommandExpected: 0,
			IsCommandExpected: false,
		},
		{
			Text: "quit",
			CommandExpected: 0,
			IsCommandExpected: false,
		},
		{
			Text: ":quit",
			CommandExpected: Quit,
			IsCommandExpected: true,
		},
		{
			Text: ":q",
			CommandExpected: Quit,
			IsCommandExpected: true,
		},
		{
			Text: ":e ",
			CommandExpected: Quit,
			IsCommandExpected: true,
		},
	}

	for _, test := range tests {
		c, isCmd := IsCommand(test.Text)

		if isCmd != test.IsCommandExpected {
			t.Errorf("%s failed. expected %v, got %v", test.Text, test.IsCommandExpected, isCmd)
		}
	
		if c != test.CommandExpected {
			t.Errorf("%s failed. expected %v, got %v", test.Text, test.CommandExpected, c)
		}

	}
}
