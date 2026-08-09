;; scrape hacker news titles: fetch + cheerio (slim build) from esm.sh
;; run with: cherry-quickjs examples/scrape.cljs
(require '["https://esm.sh/cheerio@1.0.0/slim" :as cheerio])

#_:clj-kondo/ignore
(def html (await (.text (await (js/fetch "https://news.ycombinator.com")))))

(def $ (cheerio/load html))

(doseq [el (js/Array.from (.get ($ ".titleline > a")))]
  (println "-" (.text ($ el))))
