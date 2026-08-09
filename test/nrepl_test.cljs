;; integration test for --nrepl, run with: choq test/nrepl_test.cljs
(require '["net" :as net]
         '[clojure.string :as str]
         '[clojure.test :as t :refer [deftest is testing]])

(when-not (.-setNoDelay (.-prototype net/Socket))
  (set! (.-setNoDelay (.-prototype net/Socket)) (fn [] nil)))

(def port 13399)
#_:clj-kondo/ignore
(def nrepl (await (js/import "cherry-cljs/lib/node.nrepl_server.js")))
#_:clj-kondo/ignore
(await (.startServer nrepl #js {:port port}))

;;;; bencode

(defn bencode [m]
  (str "d" (apply str (map (fn [[k v]] (str (count k) ":" k (count v) ":" v)) m)) "e"))

(defn decode-at
  "Decodes one bencode value at pos. Returns [value next-pos], or nil
  when the input is incomplete. Assumes ascii payloads."
  [s pos]
  (when (< pos (count s))
    (let [c (nth s pos)]
      (cond
        (= \i c)
        (when-let [e (str/index-of s "e" pos)]
          [(js/parseInt (subs s (inc pos) e)) (inc e)])

        (= \d c)
        (loop [m {} p (inc pos)]
          (cond
            (>= p (count s)) nil
            (= \e (nth s p)) [m (inc p)]
            :else (when-let [[k p2] (decode-at s p)]
                    (when-let [[v p3] (decode-at s p2)]
                      (recur (assoc m k v) p3)))))

        (= \l c)
        (loop [v [] p (inc pos)]
          (cond
            (>= p (count s)) nil
            (= \e (nth s p)) [v (inc p)]
            :else (when-let [[x p2] (decode-at s p)]
                    (recur (conj v x) p2))))

        :else
        (when-let [colon (str/index-of s ":" pos)]
          (let [len (js/parseInt (subs s pos colon))
                start (inc colon)
                end (+ start len)]
            (when (<= end (count s))
              [(subs s start end) end])))))))

(defn decode-all [s]
  (loop [msgs [] pos 0]
    (if-let [[m p] (decode-at s pos)]
      (recur (conj msgs m) p)
      [msgs (subs s pos)])))

;;;; client

(defn connect
  "Resolves to a client for sequential requests."
  []
  (js/Promise.
   (fn [resolve reject]
     (let [buf (atom "")
           waiter (atom nil)
           sock (net/connect port "127.0.0.1")]
       (.on sock "connect" (fn [] (resolve {:sock sock :buf buf :waiter waiter})))
       (.on sock "error" reject)
       (.on sock "data"
            (fn [d]
              (swap! buf str (.toString d))
              (let [[msgs remainder] (decode-all @buf)]
                (reset! buf remainder)
                (when-let [w @waiter]
                  (let [acc (into (:acc w) msgs)
                        done? (some (fn [m] (some #{"done"} (get m "status"))) msgs)]
                    (if done?
                      (do (reset! waiter nil)
                          ((:resolve w) acc))
                      (reset! waiter (assoc w :acc acc))))))))))))

(def id-counter (atom 0))

(defn request
  "Sends one message, resolves with all response messages up to and
  including the one with a done status."
  [client msg]
  (js/Promise.
   (fn [resolve reject]
     (reset! (:waiter client) {:resolve resolve :acc []})
     (js/setTimeout (fn [] (reject (js/Error. (str "timeout on " (get msg "op"))))) 5000)
     (.write (:sock client) (bencode (assoc msg "id" (str (swap! id-counter inc))))))))

(defn value-of [msgs] (some #(get % "value") msgs))
(defn err-of [msgs] (some #(get % "err") msgs))
(defn ex-of [msgs] (some #(get % "ex") msgs))
(defn statuses [msgs] (set (mapcat #(get % "status") msgs)))

;;;; the suite: one session end to end

#_:clj-kondo/ignore
(def client (await (connect)))

#_:clj-kondo/ignore
(def clone-resp (await (request client {"op" "clone"})))
(def session (some #(get % "new-session") clone-resp))

(defn eval! [code]
  (request client {"op" "eval" "code" code "session" session}))

#_:clj-kondo/ignore
(def results
  {:describe (await (request client {"op" "describe" "session" session}))
   :simple (await (eval! "(reduce + (range 101))"))
   :multi-form (await (eval! "(def x 1) (def y 2) (+ x y)"))
   :def-state (await (eval! "(def counter-a 41)"))
   :use-state (await (eval! "(inc counter-a)"))
   :require-ns (await (eval! "(require '[clojure.set :as cset])"))
   :use-ns (await (eval! "(sort (vec (cset/union #{1} #{2})))"))
   :error (await (eval! "(assoc 1 :k 1)"))
   :after-error (await (eval! "(+ 1 2)"))
   :load-file (await (request client {"op" "load-file"
                                      "file" "(defn loaded-fn [x] (* x 3)) (loaded-fn 14)"
                                      "session" session}))
   :complete (await (request client {"op" "complete" "prefix" "counter" "session" session}))
   :unknown (await (request client {"op" "no-such-op" "session" session}))})

(deftest clone-test
  (testing "clone returns a session id"
    (is (string? session))
    (is (re-matches #"[a-f0-9-]{36}" session))))

(deftest describe-test
  (let [ops (some #(get % "ops") (:describe results))]
    (testing "describe lists the supported ops"
      (is (contains? ops "eval"))
      (is (contains? ops "complete"))
      (is (contains? ops "load-file")))))

(deftest eval-test
  (testing "a single form returns its value"
    (is (= "5050" (value-of (:simple results))))
    (is (contains? (statuses (:simple results)) "done")))
  (testing "multiple forms in one message return the last value"
    (is (= "3" (value-of (:multi-form results)))))
  (testing "the response carries the ns"
    (is (= "user" (some #(get % "ns") (:simple results))))))

(deftest session-state-test
  (testing "defs persist across messages in a session"
    (is (= "42" (value-of (:use-state results)))))
  (testing "required namespaces persist across messages"
    (is (= "(1 2)" (value-of (:use-ns results))))))

(deftest error-test
  (testing "an eval error returns err and ex"
    (is (str/includes? (err-of (:error results)) "IAssociative"))
    (is (some? (ex-of (:error results))))
    (is (contains? (statuses (:error results)) "done")))
  (testing "the session keeps working after an error"
    (is (= "3" (value-of (:after-error results))))))

;; defs made by load-file do not persist into later evals, so the file
;; uses its own definition
(deftest load-file-test
  (testing "load-file evaluates the file contents"
    (is (contains? (statuses (:load-file results)) "done"))
    (is (= "42" (value-of (:load-file results))))))

;; completions come from the session ns-state: locally defined vars,
;; refers and aliases, not core fns
(deftest complete-test
  (let [candidates (->> (some #(get % "completions") (:complete results))
                        (map #(get % "candidate"))
                        set)]
    (testing "completions include session-defined vars"
      (is (contains? candidates "counter-a")))))

(deftest unknown-op-test
  (testing "an unknown op reports unknown-op"
    (is (contains? (statuses (:unknown results)) "unknown-op"))))

(def summary (t/run-tests))
(js/process.exit (if (pos? (+ (:fail summary) (:error summary))) 1 0))
