;; render markdown to html with markdown-it from esm.sh
;; run with: cherry-quickjs examples/markdown.cljs
(require '["https://esm.sh/markdown-it@14.1.0$default" :as MarkdownIt])

(def md (MarkdownIt.))

(def doc "# Hello cherry\n\nSome *emphasis* and a [link](https://github.com/squint-cljs/cherry).\n\n- one\n- two\n")

(println (.render md doc))
