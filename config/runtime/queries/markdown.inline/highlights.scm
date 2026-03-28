;; From nvim-treesitter/nvim-treesitter
[
  (code_span)
  (link_title)
] @markup.raw.inline

([
  (emphasis_delimiter)
  (code_span_delimiter)
] @punctuation.bracket (#conceal!))

(emphasis) @markup.italic

(strong_emphasis) @markup.bold

(strikethrough) @markup.strikethrough

[
  (link_destination)
  (uri_autolink)
] @markup.link.url

[
  (link_text)
  (image_description)
] @markup.link.text

(link_label) @markup.link.label

[
  (backslash_escape)
  (hard_line_break)
] @constant.character.escape

(image ["[" "]" "(" ")"] @punctuation.bracket)
(image "!" @punctuation.special)
(inline_link ["[" "]" "(" ")"] @punctuation.bracket)
(shortcut_link ["[" "]"] @punctuation.bracket)

((image "[" @punctuation.bracket) (#conceal!))
((image "]" @punctuation.bracket) (#conceal!))
((image "(" @punctuation.bracket) (#conceal!))
((image ")" @punctuation.bracket) (#conceal!))
((image "!" @punctuation.special) (#conceal!))
((image (link_destination) @markup.link.url) (#conceal!))
((inline_link "[" @punctuation.bracket) (#conceal!))
((inline_link "]" @punctuation.bracket) (#conceal!))
((inline_link "(" @punctuation.bracket) (#conceal!))
((inline_link ")" @punctuation.bracket) (#conceal!))
((inline_link (link_destination) @markup.link.url) (#conceal!))
((shortcut_link "[" @punctuation.bracket) (#conceal!))
((shortcut_link "]" @punctuation.bracket) (#conceal!))

