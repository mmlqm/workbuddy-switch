import { useCallback, useEffect, useState } from "react";
import { Gift, Loader2, Play, Sparkles, Trophy, Users } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import * as api from "@/lib/api";
import { asError } from "@/lib/api";
import { useAccountsStore } from "@/stores/accounts";

type TaskItem = {
  taskCode: string;
  name: string;
  acceptStatus: string | null;
  status: string | null;
  current: number | null;
  target: number | null;
};

const TASK_CN: Record<string, string> = {
  design_creative: "设计创意模式",
  explore_inspiration: "探索优秀灵感",
  desktop_chat: "桌面端对话",
  try_skill: "尝鲜热门技能",
  use_library: "体验资料库",
  tencent_cloud_expert: "腾讯轻量云专家",
  peacekeeper_theme: "和平精英主题",
  Buddy_App: "发现应用",
  Buddy_App_QQ: "企鹅教师助手",
  glm_chat: "GLM-5.2模型对话",
  chat_5_times: "和AI聊天5次",
  night_owl: "夜猫子活动",
  summon_team: "召唤3次专家团",
  summon_expert: "召唤5次专家",
  template_5: "使用5个模板",
  set_automation: "设置自动化任务",
  adopt_buddy: "领取Buddy",
  create_canvas: "创建画布",
  playbook_prompt: "使用Playbook",
  RichMeow_Chat: "RichMeow对话",
  Library_read: "阅读资料库",
  Expert_lighthouse: "专家·灯塔",
  Expert_Philanthropy: "专家·公益",
  Hp_Appearance: "设置助手外观",
  wb_wechat_oa_subscribe_task: "关注公众号",
  Model_chat_GLM5_2: "GLM-5.2对话",
  "Model_chat_GLM5.2": "GLM-5.2对话",
  black_cat: "遇见黑猫",
  Expert_team_use_3: "使用3次专家团",
  first_buddy: "领取你的Buddy",
  chat_5: "和AI对话5次",
  skill_1: "使用1个技能",
  expert_5: "召唤5次专家",
  automation_1: "设置1个自动化",
};

export default function GrowthPage() {
  const accounts = useAccountsStore((s) => s.accounts);
  const cnAccounts = accounts.filter((a) => a.variant === "cn");
  const [accountId, setAccountId] = useState<string>("");
  const [tasks, setTasks] = useState<TaskItem[]>([]);
  const [profile, setProfile] = useState<any>(null);
  const [busy, setBusy] = useState(false);
  const [log, setLog] = useState<string[]>([]);

  useEffect(() => {
    if (!accountId && cnAccounts.length > 0) {
      setAccountId(cnAccounts[0].id);
    }
  }, [cnAccounts, accountId]);

  const appendLog = useCallback((lines: string | string[]) => {
    setLog((prev) => [...prev, ...(Array.isArray(lines) ? lines : [lines])]);
  }, []);

  const loadTasks = useCallback(async () => {
    if (!accountId) return;
    setBusy(true);
    try {
      const res = await api.growthTasks(accountId);
      if (res.ok) {
        setTasks(res.tasks as TaskItem[]);
      } else {
        appendLog(`任务加载失败：${res.error || "未知"}`);
      }
      const p = await api.growthProfile(accountId);
      if (p.ok) setProfile(p);
    } catch (e) {
      appendLog(`任务查询出错：${asError(e)}`);
    } finally {
      setBusy(false);
    }
  }, [accountId, appendLog]);

  useEffect(() => {
    void loadTasks();
  }, [loadTasks]);

  const runOnce = useCallback(async () => {
    if (!accountId) return;
    setBusy(true);
    appendLog("— 开始一键执行 —");
    try {
      const res = await api.growthRun(accountId);
      appendLog([
        `接受任务：${res.accepted ?? 0}`,
        `补领：${Array.isArray(res.claimed) ? res.claimed.length : 0} 项`,
        `抽奖：${res.lottery?.drawn ?? 0} 次 ${res.lottery?.prizes?.join("、") ?? ""}`,
        `盲盒：${res.blindbox?.opened ?? 0} 个 ${res.blindbox?.items?.join("、") ?? ""}`,
      ]);
      await loadTasks();
    } catch (e) {
      appendLog(`执行出错：${asError(e)}`);
    } finally {
      setBusy(false);
    }
  }, [accountId, appendLog, loadTasks]);

  const runAll = useCallback(async () => {
    setBusy(true);
    appendLog(`— 全部 ${cnAccounts.length} 个国内版账号开始执行 —`);
    try {
      const res = await api.growthRunAll();
      const list = (res?.accounts ?? []) as any[];
      appendLog(`共处理 ${list.length} 个账号：`);
      for (const item of list) {
        const r = item.result ?? {};
        const claimed = Array.isArray(r.claimed) ? r.claimed.length : 0;
        const drawn = r.lottery?.drawn ?? 0;
        const opened = r.blindbox?.opened ?? 0;
        appendLog(
          `  ${item.account || item.accountId}: 接受${r.accepted ?? 0} 补领${claimed} 抽奖${drawn} 盲盒${opened}` +
            (r.blindbox?.items?.length ? ` (${r.blindbox.items.join("、")})` : ""),
        );
      }
      if (list.length === 0) appendLog("  （无国内版账号）");
      await loadTasks();
    } catch (e) {
      appendLog(`全部执行出错：${asError(e)}`);
    } finally {
      setBusy(false);
    }
  }, [cnAccounts.length, appendLog, loadTasks]);

  const claimAll = useCallback(async () => {
    if (!accountId) return;
    setBusy(true);
    try {
      const res = await api.growthClaimAll(accountId);
      appendLog(`补领完成：${res.claimed?.length ?? 0} 项，已领过 ${res.already ?? 0} 项`);
      await loadTasks();
    } catch (e) {
      appendLog(`补领出错：${asError(e)}`);
    } finally {
      setBusy(false);
    }
  }, [accountId, appendLog, loadTasks]);

  const lottery = useCallback(async () => {
    if (!accountId) return;
    setBusy(true);
    try {
      const res = await api.growthLottery(accountId, 50);
      appendLog(`抽奖 ${res.drawn} 次：${res.prizes?.join("、") ?? "无"}`);
    } catch (e) {
      appendLog(`抽奖出错：${asError(e)}`);
    } finally {
      setBusy(false);
    }
  }, [accountId, appendLog]);

  const statusBadge = (t: TaskItem) => {
    const acc = t.acceptStatus || "";
    if (acc === "claimed") return <Badge variant="outline">已领奖</Badge>;
    if (acc === "not_accepted") return <Badge>未接受</Badge>;
    if (t.target && t.current != null && t.current >= t.target)
      return <Badge variant="secondary">可领奖</Badge>;
    return <Badge variant="outline">进行中</Badge>;
  };

  const taskName = (t: TaskItem) => {
    if (t.name && t.name !== t.taskCode) return t.name;
    return TASK_CN[t.taskCode] || t.taskCode;
  };

  return (
    <div className="mx-auto max-w-4xl space-y-4 p-6">
      <div>
        <h1 className="text-xl font-semibold tracking-tight">成长中心</h1>
        <p className="text-sm text-muted-foreground">
          接受任务、补领奖励、大转盘抽奖与盲盒（仅国内版账号）
        </p>
      </div>

      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Sparkles className="size-4" /> 账号操作
          </CardTitle>
          <CardDescription>选择一个国内版账号执行</CardDescription>
        </CardHeader>
        <CardContent className="space-y-3">
          <div className="flex flex-wrap items-center gap-2">
            <select
              value={accountId}
              onChange={(e) => setAccountId(e.target.value)}
              className="h-9 rounded-md border bg-transparent px-3 text-sm"
            >
              {cnAccounts.map((a) => (
                <option key={a.id} value={a.id}>
                  {a.nickname || a.email || a.id}
                </option>
              ))}
            </select>
            <Button onClick={runAll} disabled={busy}>
              {busy ? <Loader2 className="size-4 animate-spin" /> : <Users className="size-4" />}
              全部账号执行（{cnAccounts.length}）
            </Button>
            <Button onClick={runOnce} disabled={busy || !accountId}>
              {busy ? <Loader2 className="size-4 animate-spin" /> : <Play className="size-4" />}
              一键执行
            </Button>
            <Button variant="secondary" onClick={claimAll} disabled={busy || !accountId}>
              <Trophy className="size-4" /> 补领
            </Button>
            <Button variant="secondary" onClick={lottery} disabled={busy || !accountId}>
              <Gift className="size-4" /> 抽奖
            </Button>
          </div>
          {profile?.ok && (
            <div className="flex gap-4 text-sm text-muted-foreground">
              <span>等级：{profile.level ?? "-"}</span>
              <span>积分：{profile.points ?? "-"}</span>
              <span>能量：{profile.energy ?? "-"}</span>
              <span>连签：{profile.streakDays ?? "-"} 天</span>
            </div>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>任务列表</CardTitle>
        </CardHeader>
        <CardContent>
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b text-left text-muted-foreground">
                <th className="py-2 pr-4 font-medium">任务</th>
                <th className="py-2 pr-4 font-medium">进度</th>
                <th className="py-2 font-medium">状态</th>
              </tr>
            </thead>
            <tbody>
              {tasks.map((t) => (
                <tr key={t.taskCode} className="border-b last:border-0">
                  <td className="py-2 pr-4">{taskName(t)}</td>
                  <td className="py-2 pr-4 text-muted-foreground">
                    {t.target ? `${t.current ?? 0} / ${t.target}` : `${t.current ?? 0}`}
                  </td>
                  <td className="py-2">{statusBadge(t)}</td>
                </tr>
              ))}
              {tasks.length === 0 && (
                <tr><td colSpan={3} className="py-4 text-center text-muted-foreground">暂无任务</td></tr>
              )}
            </tbody>
          </table>
        </CardContent>
      </Card>

      {log.length > 0 && (
        <Card>
          <CardHeader><CardTitle>运行日志</CardTitle></CardHeader>
          <CardContent>
            <pre className="max-h-64 overflow-auto whitespace-pre-wrap text-xs text-muted-foreground">
              {log.join("\n")}
            </pre>
          </CardContent>
        </Card>
      )}
    </div>
  );
}
