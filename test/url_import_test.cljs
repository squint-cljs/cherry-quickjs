;; url import integration test; loaded by test/run_tests.cljs
;; exercises the readme examples: esm.sh downloads and node builtin
;; mapping in remote modules
(require '["https://esm.sh/@babashka/cli" :as cli]
         '["https://esm.sh/@babashka/fs" :as bfs]
         '["https://esm.sh/lodash-es@4.17.21" :as l]
         '[clojure.string :as str]
         '[clojure.test :as t :refer [deftest is testing]])

(deftest babashka-fs-test
  (testing "a squint lib using node builtins works"
    (is (true? (bfs/exists? "README.md")))
    (is (str/starts-with? (bfs/slurp "README.md") "# Choq"))))

(deftest babashka-cli-test
  (testing "parse-args parses options"
    (is (= 1 (.. (cli/parse-args #js ["--foo" "1"]) -opts -foo)))))

(deftest lodash-test
  (testing "a plain esm lib works"
    (is (= "fooBar" (l/camelCase "foo bar")))))
