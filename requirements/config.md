# OJ 系统配置文件定义

## 配置格式

OJ 系统使用本地单个 JSON 文件进行配置，其样例如下：

```json
{
  "server": {
    "bind_address": "127.0.0.1",
    "bind_port": 12345
  },
  "problems": [
    {
      "id": 0,
      "name": "aplusb",
      "type": "standard",
      "misc": {},
      "cases": [
        {
          "score": 50.0,
          "input_file": "./data/aplusb/1.in",
          "answer_file": "./data/aplusb/1.ans",
          "time_limit": 1000000,
          "memory_limit": 1048576
        }, {
          "score": 50.0,
          "input_file": "./data/aplusb/2.in",
          "answer_file": "./data/aplusb/2.ans",
          "time_limit": 1000000,
          "memory_limit": 1048576
        }
      ]
    }
  ],
  "languages": [
    {
      "name": "Rust",
      "file_name": "main.rs",
      "command": ["rustc", "-C", "opt-level=2", "-o", "%OUTPUT%", "%INPUT%"]
    }
  ]
}
```

配置文件中以 JSON 格式保存了一个字典，其字段如下：

* `server`：必选，保存了服务器相关配置，其中：
    * `bind_address`：可选，HTTP 服务器绑定的地址（默认为 `127.0.0.1`）
    * `bind_port`：可选，HTTP 服务器绑定的端口（默认为 `12345`）
* `problems`：必选，记录了所有的题目的数组，数组每个元素是一个字典，每个字典对应一个题目
* `languages`：必选，记录了所有编程语言的数组，数组每个元素是一个字典，每个字典对应一个编程语言

每个题目对应一个字典，其字段如下：

1. `id`：必选，每个题目都有唯一的 ID，不保证顺序和连续；
2. `name`：必选，题目名称；
3. `type`：必选，题目类型，可能出现的值有 `standard`（标准题，比较时忽略文末空行和行末空格）、`strict`（标准题，严格对比输出和答案）、`spj`（标准题，使用 Special Judge 对比输出）、`dynamic_ranking`（竞争得分题，使用 standard 模式对比输出，并根据指标竞争得分）；
4. `misc`：可选，根据题目类型附加额外的信息，在实现部分提高要求时会涉及；
5. `cases`：必选，一个记录了所有数据点的数组，数据点按顺序从 1 开始编号，每个数据点是一个字典，有如下的字段：
    1. `score`：必选，该数据点的分数，可以有小数；
    2. `input_file`：必选，该数据点的输入文件；
    3. `answer_file`：必选，该数据点的答案文件；
    4. `time_limit`：必选，该数据点的时间限制，正整数，单位是 us；
    5. `memory_limit`：必选，该数据点的内存限制，非负整数，单位是字节，0 表示不限制。

每种编程语言对应一个字典，其字段如下：

1. `name`：必选，编程语言名称；
2. `file_name`：必选，保存待评测代码的文件名；
3. `command`：必选，一个数组，数组的第一项是所使用的编译器，其余是其命令行参数，其中如果出现了一项为 `%INPUT%`，则要替换其为源代码路径，如果出现了一项为 `%OUTPUT%`，则要替换其为可执行文件路径。

保证所有数据点的分数之和为 100。

如果使用 `serde_json` 来结构化解析配置文件，由于 `type` 是关键字，如果直接写 `type: ProblemType` 会报错；这里可以用 `serde` 的标注来解决这个问题：

```rust
#[serde(rename = "type")]
ty: ProblemType,
```

详细见 [serde 的文档](https://serde.rs/attributes.html)。

## 样例解析

对于上面的配置文件样例，OJ 系统如果接受到评测任务，首先根据评测 ID 找到题目配置，根据编程语言找到编程语言配置。接着，做下面几件事情：

1. 创建一个评测临时目录，用于保存评测时使用的源代码、可执行文件和输出文件，下面用 `TMPDIR` 来指代这一步创建的临时目录；
2. 将源代码 `source_code` 字段的内容保存到临时目录中，文件名为编程语言配置中的 `file_name` 字段（例如，如果提交时设置了 Rust 语言，那么就把源代码保存在 `TMPDIR/main.rs`）；
3. 根据编程语言配置，将源代码编译成可执行文件，需要你自己确定一个可执行文件的名字（Windows 下运行还需要保证后缀名为 `.exe`）。假如提交时设置了 Rust 语言，自己定的可执行文件叫做 `test.exe`，那么这里运行的命令可能是 `rustc -C opt-level=2 -o TMPDIR/test.exe TMPDIR/main.rs`；
4. 编译成功后，按照顺序对数据点进行评测，运行可执行程序，将其标准输入重定向为 `input_file`，标准输出重定向到一个临时文件，运行结束后将临时文件与答案文件的内容比对。以上面的 `aplusb` 题目为例子，那么运行的命令相当于 `TMPDIR/test.exe < ./data/aplusb/1.in > TMPDIR/test.out`，然后再比对 `TMPDIR/test.out` 与 `./data/aplusb/1.ans` 的内容；
5. 完成评测后，可以删除评测临时目录中的所有内容。

为了保证多个评测可以同时进行，需要保证临时目录不会冲突。也请注意不要把临时目录中的文件提交到仓库中。
