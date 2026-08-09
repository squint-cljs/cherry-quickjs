;; integration test for --nrepl, run with: cherry-quickjs test/nrepl_test.cljs
(require '[clojure.test :as t :refer [deftest is]]
         '[clojure.string :as str]
         '["net" :as net])

(when-not (.-setNoDelay (.-prototype net/Socket))
  (set! (.-setNoDelay (.-prototype net/Socket)) (fn [] nil)))

(def port 13399)
(def nrepl (await (js/import "cherry-cljs/lib/node.nrepl_server.js")))
(await (.startServer nrepl #js {:port port}))

(defn bencode [m]
  (str "d" (apply str (map (fn [[k v]] (str (count k) ":" k (count v) ":" v)) m)) "e"))

(defn exchange
  "Connects, clones a session, evals code in it. Resolves to a js array
  of the raw clone and eval responses."
  [code]
  (js/Promise.
   (fn [resolve reject]
     (let [state (atom {:step :clone :clone "" :eval ""})
           sock (net/connect port "127.0.0.1")]
       (.on sock "connect" (fn [] (.write sock (bencode {"op" "clone" "id" "1"}))))
       (.on sock "data"
            (fn [d]
              (let [s (.toString d)]
                (if (= :clone (:step @state))
                  (do (swap! state update :clone str s)
                      (when-let [[_ sess] (re-find #"new-session(?:\d+):([a-f0-9-]+)" (:clone @state))]
                        (swap! state assoc :step :eval)
                        (.write sock (bencode {"op" "eval" "code" code "session" sess "id" "2"}))))
                  (do (swap! state update :eval str s)
                      (when (str/includes? (:eval @state) "4:done")
                        (.end sock)
                        (resolve #js [(:clone @state) (:eval @state)])))))))
       (.on sock "error" reject)
       (js/setTimeout (fn [] (reject (js/Error. "timeout"))) 5000)))))

(def result (await (exchange "(reduce + (range 101))")))
(def clone-resp (aget result 0))
(def eval-resp (aget result 1))

(def err-result (await (exchange "(assoc 1 :k 1)")))
(def err-resp (aget err-result 1))

(deftest clone-returns-session
  (is (str/includes? clone-resp "new-session")))

(deftest eval-returns-value
  (is (str/includes? eval-resp "5:value4:5050"))
  (is (str/includes? eval-resp "4:done")))

(deftest eval-error-returns-err
  (is (str/includes? err-resp "3:err"))
  (is (str/includes? err-resp "IAssociative"))
  (is (str/includes? err-resp "4:done")))

(def summary (t/run-tests))
(js/process.exit (if (pos? (+ (:fail summary) (:error summary))) 1 0))
